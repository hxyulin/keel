# x86 xstate native test programs

The `x86_xstate` test in `x86_simd_intrinsics` checks Keel's XSAVE/XRSTOR-family
code on real x86-64 hardware. Each configuration is a `native_program!`: it runs
on x86-64 Linux with XSAVE and AVX, and `KEEL_TEST_EXPORT_DIR` exports it for
`suite/integration/x86-exports/run.py`. The test also checks the opcode bytes
against Clang and GNU as, and the Clang builtins against the forced opcodes.

No privileged instructions or permission changes are used. The complete
CPUID(D,0).EBX allocation accommodates hosts enabling AVX512/AMX, but every
instruction requests only baseline bits 0..2. No whole-state closure claim.

18 objects: 3 selectors x 2 allocators x normal/spill/single-SIMD custom pools.
A checked public allocator adapter delegates to Fast/Full and checks clobber
classification and live-load spill placement. This is not proof of arbitrary
third-party allocator plans. Their liveness obligations remain part of the API.

Per object: 7 masks (0,1,2,3,4,6,7), 28 patterned save/restore cases and init/partial
checks against forced opcodes and Clang builtins; 57,344 live scalar/vector+GPR
calls with unaligned inputs and output guards; 8 subprocess fault cases; valid
CPUID-size buffers ending at protected pages; explicit raw MXCSR sequencing and
8 raw SSE overflow/rounding cases ordered restore -> ADDSS -> save -> MXCSR read.
Save probes seed all 16 XMM/YMM registers and all 8 x87 data registers using a
trusted assembly boundary. Restore inputs satisfy effective empty x87 stack,
TOP0 and masked exceptions. Undefined padding and empty x87 payload are excluded
from comparisons. Live-save tests search the saved register image for the loaded
SSA bits, so entry-state capture/canonicalization cannot substitute for capture
at the compiler instruction boundary.

The standalone assembly seed/probe obeys SysV GPR preservation and stack alignment;
it resets the x87/MXCSR environment before returning to C. Raw environment functions
are called deliberately by that assembly probe. Restore callers must balance control
state at ordinary foreign ABI boundaries.
