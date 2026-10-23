# Scalar x86 port I/O

IDs 717–728 append `x86_in8/16/32`, `x86_out8/16/32`, then the corresponding
`_imm` forms, preserving all previous IDs. IN takes an I16 port and returns
I8/I16/I32; OUT takes an I16 port followed by same-width data and returns void.
Dynamic forms always use DX. `_imm` requires a literal I16 constant 0..255;
invalid values are rejected without masking. Each operation transfers exactly
1, 2, or 4 bytes. No 64-bit or string I/O form is provided.

All forms are pinned barriers, including dead IN results: permission faults and
device effects remain observable. They are baseline x86 (NONE/V1/V2/V3), with
explicit AArch64 rejection. Generic memory and integer semantics are unchanged.
No FLAGS-region preservation or host permission guarantee is made.

The caller must supply valid CPL/IOPL/TSS I/O-bitmap permissions covering the
entire access and a device contract for its port, width, and effects. There is
no permission probing, IOPL enabling, or emulation. Fault handling is terminal
only; arbitrary resumption is outside this contract.

Primary authority: Intel SDM 093, IN Vol.2A 3-455–456, OUT Vol.2B 4-171–172,
from `suite/evidence/full-scope-contract-audit-20261023/primary.tar.gz`, member
`primary/intel-current.pdf`, SHA256
`a4a62e6a7ba11a76c7753a195b825087306812aac39b930973f9168ee599f321`.
Official pinned source:
https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf
The instruction tables specify E4/E5/EC/ED and E6/E7/EE/EF, operand-size prefix
66 for word accesses, and no FLAGS affected by the principal instruction.

Runtime proof is reserved to the root owner: only a disposable root-permitted
guest with QEMU pc-testdev at E0..E3 (byte/word/dword latch), and explicit root
permission. Compile-only tests do not establish device or permission behavior.
