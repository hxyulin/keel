# Remaining x86 system instruction audit

Audit date: 2026-10-25. Source checkpoint: `151e7ac7`.

The later [Linux kernel readiness audit](linux-kernel-readiness.md) checks
selected instruction bindings, GNU assembly syntax, C frontend behavior and
build-driver gaps at `2da2e6a6`. It proposes a configured kernel target rather
than treating typed system APIs as a prerequisite for compiling Linux assembly.

Follow-up: [SYSEXIT/RSM assembler support](x86-system-exits.md) closes the two
assembler exposure gaps below and adds width-specific formatting. Its explicit
RSM policy also resolves the I486 classification gap. Terminal compiler
contracts and execution qualification remain open. The findings and ledger
digest below record the original audit checkpoint.

The later [query/I/O binding migration](x86-query-bindings.md) generates 42
existing binding tables. Its baseline probe found assembly failures in the
32/64-bit register LAR/LSL paths. A follow-up fixes those four APIs by using
the destination-width register view for the selector. All 42 query/I/O bindings
now pass the compile/assembly matrix. Descriptor-query execution qualification
remains open.

Most remaining baseline work concerns compiler contracts and qualification,
rather than missing instruction bytes. Two public assembler gaps are confirmed:
**SYSEXIT and RSM**. Their forms already exist in the shared catalogue. Optional
RDTSCP has the same exposure gap. None of these findings changes production
support in this audit.

## Boundary and method

The project baseline system goal extends beyond the psABI userspace feature
level. The [psABI feature table](https://gitlab.com/x86-psABIs/x86-64-ABI/-/raw/master/x86-64-ABI/low-level-sys-info.tex)
defines processor feature levels, not a kernel entry or saved-state ABI.
This audit uses the project's existing system scope, including its SMM and
vendor-qualified transition backlog. It does not redefine v1 to include every
optional system extension.

The [read-only inventory tool](../tools/x86-isa/audit_system.py) joins a manual
roster to the [coverage ledger](x86-coverage/README.md), using original operands
and far-transfer flags to distinguish system subforms of MOV/CALL/JMP/RET and
PUSH/POP. It checks the ledger's input hashes before reporting. It emits source
IDs, source/table indices, decoder registration, candidate links and policy
classification for every selected record. Reproduce it with:

```sh
python3 tools/x86-isa/generate.py --check
python3 tools/x86-isa/audit_system.py > /tmp/keel-system-audit-inventory.json
```

The ledger SHA-256 at this checkpoint is
`d4d6639130b6db77ad9cc1d8a545f58e9c02558dde62e1db0393b092ffbfa520`.
The roster contains **122 imported records, 118 in the production catalogue**.
The four exclusions are alternate REX2 FX context records. These are source
record counts, not unique instructions, operand combinations or support rates.
All 118 included records have enabled decoder registrations; this does not
substitute for byte witnesses.

The roster is intentionally broader than strictly privileged instructions:
descriptor queries, I/O and flags depend on current machine permissions, and
CPUID/context operations matter to kernel state ownership. Ordinary arithmetic,
all x87 operations and all optional kernel-useful extensions are not enumerated
as baseline gaps. Legacy-only instructions absent from the long-mode import
are outside this audit. It is not a proof that the pinned import covers the
complete architectural manuals.

## Existing compiler paths

| Family | Imported / catalogue records | Current compiler path | Remaining work |
|---|---:|---|---|
| SGDT/SIDT/LGDT/LIDT | 4 / 4 | Typed queries; generated load recipes | Broader machine-state/fault qualification. |
| SLDT/STR/SMSW | 6 / 6 | Typed width-specific queries | Extend finite qualified states as needed. |
| LAR/LSL/VERR/VERW | 8 / 8 | Handwritten typed query lowering | CPL0 qualification with valid/invalid selectors and result/ZF checks. |
| LLDT/LTR/LMSW/INVLPG | 7 / 7 | Generated register/memory/address recipes | Broader descriptor, CR0 and translation-context qualification. |
| CLTS/INVD/WBINVD | 3 / 3 | Generated fixed recipes | Native cache behavior and multicore scope remain unqualified. |
| RDMSR/WRMSR | 2 / 2 | Generated declarations, handwritten fixed-register hooks | MSR-specific preconditions; more index/state qualification. |
| MOV CR/DR | 4 / 4 | Generated per-register recipes | Further mode, address-space, breakpoint and fault contracts. |
| CLI/STI/HLT | 3 / 3 | Generated fixed/adjacent-pair recipes | Existing IRQ fixture is bounded; no general handler ABI. |
| SWAPGS | 1 / 1 | Generated fixed recipe | Complete entry/exit sequencing remains caller-owned. |
| IN/OUT | 16 / 16 | Handwritten typed scalar I/O lowering | Owned device/permission execution fixture. |
| INSB/W/D, OUTSB/W/D | 24 / 24 | Handwritten typed string/REP I/O lowering | Owned device, DF, count, memory-footprint and permission qualification. |
| CPUID/RDTSC/RDPMC | 3 / 3 | Handwritten fixed-register query lowering | Performance-counter availability/permission and ordering stay explicit. |
| PUSHF/PUSHFQ/POPF/POPFQ | 6 / 6 | Raw RFLAGS interfaces and owned regions | Full privileged flag restoration is not a generic returning-call guarantee. |
| FXSAVE/FXSAVE64/FXRSTOR/FXRSTOR64 | 8 / 4 | Typed 64-bit context APIs | Broader state ownership/restoration qualification; not all context is covered. |

These interfaces were checked in `intrinsic.rs`, the native backend dispatch,
`cpu_control.rs`, and the system recipe schema. The ledger only links its 45
declared system recipes. Empty links for older queries, port I/O, flags and
context operations do **not** mean missing compiler support.

Historical bounded execution evidence is recorded in
[system support](x86-system-support.md). No privileged guest campaigns or native
privileged qualification were rerun for this audit. Physical cache effects and descriptor
query/port-I/O qualification are not promoted to verified support here.

## Missing contracts and assembler exposure

| Family | Catalogue records | Public assembly | Compiler gap | Generation fit |
|---|---:|---|---|---|
| IRET/IRETD/IRETQ | 3 | `iretw`, `iretd`, `iretq` | Owned IRETQ adapter exists; no equivalent owned 16/32-bit return contract. | Names/bytes already generated. Frame/mode contracts need explicit adapters. |
| SYSCALL | 1 | `syscall` | Returning Linux transport exists. General OS entry, first-instruction capture and stack acquisition do not. | Existing form suffices; entry shim and protocol need design. |
| SYSRET | 2 | `sysret`, `sysretq` | No owned terminal return API. | Existing forms suffice. A target image, selector/MSR policy, stack installation and fault contract remain explicit. |
| SYSENTER | 1 | `sysenter` | No general entry shim or caller protocol. | Existing form suffices; vendor/mode contract first. |
| SYSEXIT | 1 | **Missing**; `sysexit`, `sysexitq`, `sysexitl` reject. | No terminal return contract. | Generate identity and deliberate width selection over the imported form; do not invent another encoder. |
| INT/INT1/INT3 | 3 | `int 42`, `int1`, `int3`; `int.imm` also exists. | `debugtrap` emits INT3; arbitrary-vector transport and INT1 have no owned native interface. | An immediate-vector recipe needs validation and handler clobber/continuation rules. Terminal delivery needs a different contract. |
| Far CALL/JMP | 2 | `lcallq [rax]`, `ljmpq [rax]` accepted. | No owned descriptor/privilege/stack transfer API. | Existing forms and Core adapters suffice; surrounding state transition is explicit. |
| Far RET | 2 | `retfw`, `retf`, `retfq` accepted. | No owned return-frame API. | Existing forms suffice; target and stack ownership differ from ordinary RET. |
| LFS/LGS/LSS | 3 | 64-bit GPR plus memory accepted. | No typed segment-cache/stack-shadow contract. | A new stateful recipe could handle operands; it must model resulting segment state and continuation. |
| MOV segment | 4 | `mov ax, ds` and `mov ds, ax` accepted. | No general typed segment access contract. | Register/operand definitions exist; effects and valid destination policy need review. |
| PUSH/POP FS/GS | 4 | Both FS and GS accepted. | No typed segment stack/cache API. | Existing forms suffice; direct stack mutation cannot be an unconstrained ordinary intrinsic. |
| RSM | 1 | **Missing**; `rsm` rejects. | No owned SMM saved-state/terminal API or SMM campaign. | Fixed identity is simple. Existing ordinary fixed lowering is insufficient for terminal semantics. |

The bare `iret` spelling rejects in the current public parser; `iretw` is its
explicit 16-bit spelling. That is an alias choice, not a missing catalogue form.
The earlier transition table incorrectly grouped SYSEXIT with exposed fixed
identities. This audit corrects that claim.

These assembly probes establish acceptance of exactly the listed examples,
not every width, address form, privilege/mode combination or correct execution.
Separate literal-byte objects decode as SYSEXIT (`0f 35` and `48 0f 35`), RSM
(`0f aa`) and RDTSCP (`0f 01 f9`). The decoder currently prints `sysexit` for
both width witnesses. Adding its public adapter must address width-bearing
formatting before claiming textual round trips. These observations do not
qualify execution.
The owned [IRETQ adapter](x86-iretq.md) and [transition constraints](x86-system-transitions.md)
remain the implementation references. Architectural review should use the
instruction entries and system chapters in the
[Intel manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html),
including vendor distinctions already recorded in the transition audit.

## Optional system families

As a cross-check, the tool also selects ledger categories SYSTEM, SYSRET,
SYSCALL, INTERRUPT, SEGOP, IO and IOSTRING, plus every explicit CPL0 record.
This sweep finds **131 records across 95 source mnemonics**, with 126 records
in the catalogue. Unknown CPL is not interpreted as unprivileged. Categories
are import classifications, so the sweep alone misses some relevant forms,
including IRET and CLI/STI; the explicit roster covers them.

The following sets surfaced outside the baseline roster. Counts include
excluded records. Their presence is not a proposal to enable them under v1.

| Imported ISA label | Records | Mnemonics surfaced |
|---|---:|---|
| AMD_INVLPGB | 2 | INVLPGB, TLBSYNC |
| CET | 5 | CLRSSBSY, SETSSBSY, WRUSSD, WRUSSQ |
| FRED | 2 | ERETS, ERETU |
| HRESET | 1 | HRESET |
| INVPCID | 1 | INVPCID |
| KEYLOCKER | 1 | LOADIWKEY |
| LKGS | 2 | LKGS |
| MONITOR | 2 | MONITOR, MWAIT |
| MSRLIST | 2 | RDMSRLIST, WRMSRLIST |
| PBNDKB | 1 | PBNDKB |
| PCONFIG | 1 | PCONFIG |
| RDTSCP | 1 | RDTSCP |
| SGX | 1 | ENCLS |
| SMAP | 2 | CLAC, STAC |
| SMX | 1 | GETSEC |
| SNP | 4 | PSMASH, PVALIDATE, RMPADJUST, RMPUPDATE |
| SVM | 8 | CLGI, INVLPGA, SKINIT, STGI, VMLOAD, VMMCALL, VMRUN, VMSAVE |
| TDX | 4 | SEAMCALL, SEAMOPS, SEAMRET, TDCALL |
| VTX | 17 | INVEPT, INVVPID, VMCLEAR, VMLAUNCH, VMPTRLD/ST, VMREAD/WRITE, VMRESUME, VMXOFF/ON |
| WRMSRNS | 1 | WRMSRNS |
| XSAVE / XSAVES | 5 | XSETBV, XRSTORS/64, XSAVES/64 |

Optional families outside the category/CPL sweep also matter to kernels, such
as FSGSBASE and other cache/state extensions. The full ledger is the inventory
for those; the table is not a claim to enumerate all optional kernel features.
Local UDB also appears in the sweep and is deliberately excluded from the
architectural baseline roster.

Feature policy is a separate gap: system labels I286REAL, I286PROTECTED,
I486, PENTIUMREAL and RDPMC remain unclassified. Optional labels above also
need explicit review. Current missing-policy fallback to `Features::NONE`
does not justify advertising those instructions as baseline. Do not bulk-map
all missing keys to an empty requirement set. CPU capability, OS enablement,
privilege and execution protocol require separate decisions.

## Recommended implementation order

1. Expose SYSEXIT and RSM assembler identities through existing schemas, with
   explicit size/formatting policy and independent literal/GNU/Clang witnesses. Keep
   terminal compiler APIs separate. Decide whether the `iret` alias is wanted;
   it is not required to expose the existing form.
2. Review system feature-policy labels. Keep optional instructions out of v1
   merely by accident of a missing key. RDTSCP and SMAP are possible separate
   extension increments, not baseline completion requirements.
3. Qualify existing LAR/LSL/VERR/VERW and scalar/string I/O APIs in owned CPL0
   fixtures. This closes evidence gaps before creating duplicate interfaces.
4. Define returning arbitrary-vector INT transport, then implement immediate
   validation, preservation rules and independent handler/continuation tests.
   Introduce only the recipe needed by that contract.
5. Implement owned SYSRET return and SYSCALL entry as separate adapters with
   explicit state images, stack rules and invalid controls. Follow with
   vendor-qualified SYSENTER/SYSEXIT and far/segment transfers.
6. Leave RSM execution qualification for a dedicated SMM environment. A fixed
   opcode adapter alone must not count as completed SMM support.

All baseline families examined here already have catalogue forms; this audit
found no need for another instruction-specific encoder. Operand matcher
limitations may still emerge when broader
witnesses are added. The generator removes declarations and regular emission
duplication; it cannot derive the missing machine-state protocols from names
or CPL fields.

## Audit verification

This session passed all 79 ISA generator tests, both generator freshness
checks, inventory determinism/accounting checks and documentation link checks.
CLI assembly probes confirmed the acceptance/rejection examples above; four
literal-byte decode witnesses confirmed recognition without execution.
The full workspace test run passed 2,711 tests, with zero failures and 13
ignored across 497 reported groups. Logs are
`/tmp/keel-system-audit-workspace.log` and `/tmp/keel-system-audit-python.log`.
These checks do not populate per-form verdicts in the coverage ledger or
extend the privileged execution campaigns.
