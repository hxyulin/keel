# x86 system transitions

Transition bytes and a compiler-owned entry/return ABI are different APIs.
Keel assembles the imported system forms; only documented transports own the
register and stack restoration required to execute a terminal transition.

## Supported contracts

- The Linux syscall adapter provides returning 64-bit Linux syscall transport.
  It does not own kernel entry, child entry or arbitrary stack switching.
- The [IRETQ adapter](iretq.md) consumes an explicit image and transfers
  without returning to the compiled caller.
- [SWAPGS](swapgs.md) is an explicit operation with caller-owned GS state.
- [SYSEXIT and RSM](exits.md) have public assembly forms. Assembly
  support alone does not supply terminal IR or an entry/return ABI.

## Limits

Other transition families, arbitrary software-interrupt vectors, far/segment
transfers and SMM resume need their own state and execution contracts. An
ordinary returning intrinsic, `noreturn` annotation or replacement RET mnemonic
cannot supply those contracts.

A terminal adapter must specify its input image, restore every promised state
component, install the target stack/frame and transfer without later compiler
spills or epilogues. CPU model, privilege, mappings, segment state and asynchronous
entry remain explicit preconditions. General `naked` or `interrupt` C attributes
are not a substitute for such an adapter.
