# Remaining x86 system transitions

This audit separates existing interfaces from the contracts still needed to
complete baseline system support. Opcode recognition does not establish a
compiler entry or return ABI.

| Area | Current interface | Remaining contract |
|---|---|---|
| Returning Linux SYSCALL | `x86_64::linux_syscall::{signature, program, assembly}` transports syscall number, six arguments and raw RAX through an ordinary imported C function. | Arbitrary OS entry, stack replacement and non-returning syscalls need a separate protocol. |
| IRETQ | Fixed assembler/decoder identity; the IRQ fixture uses a trusted assembly handler. | Owned architectural frame, target GPR state, stack installation and terminal control flow. |
| SYSRET/SYSEXIT | Fixed assembler identities. | Target registers and stack, configured selector/MSR policy, CPU/mode preconditions and fault delivery after stack replacement. |
| SYSENTER/general SYSCALL entry | Assembler identities. | First-instruction entry shim, physical stack acquisition and register capture before ordinary compiled code. CPU/vendor availability remains explicit. |
| Far transfers and segment loads | Core assembler grammar includes far calls/jumps/returns and LFS/LGS/LSS; MOV/POP segment forms also exist. | Descriptor/privilege rules, segment cache effects, interrupt shadows and a valid continuation or terminal transfer. |
| RSM | Imported catalogue form is present, with absent CPL metadata. No public RSM mnemonic adapter is currently exposed. | Adapter identity plus a real SMM saved-state and terminal execution contract. |

## Constraints found in current code

[`InstData::is_terminator`](../crates/keel-ir/src/inst.rs) recognizes ordinary
branches, returns, tail transfers, invokes and traps. An intrinsic is not a
terminator. [`FunctionAttrs`](../crates/keel-ir/src/function.rs) has a
`noreturn` promise, but no naked-entry or physical-stack ownership attribute.
Those existing interfaces do not establish an interrupt-return ABI.

The [native x86 backend](../crates/keel-codegen/src/x86_64.rs) explicitly rejects
inline-assembly IR. Its ordinary epilogue restores the compiled function's
callee saves and RSP/RBP. Its `trap` terminator emits UD2. Replacing an ordinary
RET mnemonic with IRETQ/SYSRET would therefore leave the wrong stack and
register contract. Inline assembly is not an implemented escape hatch here.

The [Linux syscall adapter](../crates/keel-codegen/src/x86_64/linux_syscall.rs)
already states its limits: native Linux 64-bit returning transport, original
user stack retained, and caller-save RCX/R11/flags. It is not x32, an arbitrary
OS ABI or a child-entry/stack-switch protocol.

## Requirements for the next implementation

A terminal return needs an owned, readable target-state image whose layout
and exact access footprint are public. It must restore every promised GPR,
install the architectural return frame or target stack, and transfer without
subsequent spills, helper calls or the ordinary epilogue. Its IR use must stop
the current path. Target SIMD/x87 state, segment bases, mappings, asynchronous
entry and any additional context component must be explicitly owned rather
than implied by a GPR image.

The existing linkable typed-assembly adapter pattern offers one implementation
route: a `noreturn` declaration and transport that owns the final restoration
sequence. Another route is explicit terminal IR. Either must prove the same
stack and control-flow properties; an ordinary void intrinsic alone does not.
No target-state layout or terminal API is declared stable by this audit.

IRET and SYSRET need distinct frame contracts. SYSRET consumes RCX/R11, derives
selectors from STAR and does not load the target stack pointer. Canonicality
and exception-stack handling remain concrete caller obligations. Mode and
privilege variants must retain their architectural differences. RSM needs
an SMM test environment and saved-state ownership; an ordinary CPL0 return
fixture cannot qualify it.

Future fixtures must observe the actual target privilege, RIP, RSP, flags and
promised registers from an independent landing shim, with invalid controls
that cannot pass by faulting or timing out. Existing IRQ-frame observations
and GS exchange witnesses provide components for that validation, not a
completed entry/return implementation.

Architectural distinctions follow Intel's [instruction reference A-L](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf)
and [instruction reference M-U](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf).

## Experimental IRETQ witness

[`iret_context_reference.S`](../suite/x86-cpl0/cases/iret_context_reference.S)
restores all fifteen non-RSP GPRs from an owned image, installs its five-word
IRETQ frame, and lands on a distinct stack at CPL0. The landing shim captures
registers, RSP and flags before comparison. It then restores the guest harness
stack. This fixture defines no public context-image layout or compiler API.

Assemble the fixture with the same GNU toolchain used by the runner, then run:

```sh
x86_64-unknown-linux-gnu-as --64 \
  -o /tmp/iret-reference.o suite/x86-cpl0/cases/iret_context_reference.S
python3 suite/x86-cpl0/run.py --payload /tmp/iret-reference.o --timeout 5
```

The reference passes under QEMU 11.1.2 TCG. Reassembling separately with
`--defsym KEEL_OMIT_R10=1`, `--defsym KEEL_USE_RET=1` or
`--defsym KEEL_WRONG_FLAGS=1` produces three normally returning failures with
exact READY/FAIL and exit 35. These controls check missing register restoration,
ordinary RET in place of IRETQ, and an incorrect restored carry flag. No
privilege transition, CPL3 landing, fault recovery or compiler-generated
terminal transport is qualified by this experiment.
