# x86 xstate native test packet

The `x86_xstate` test generates a self-contained packet of objects, sources and
a runner for checking Keel's XSAVE/XRSTOR-family code on real x86-64 hardware.
Export the packet with:

    KEEL_X86_XSTATE_EXPORT_DIR=/absolute/packet cargo test -p keel-codegen --test x86_xstate -- --nocapture

Then run it on native x86-64 Linux:

    python3 /absolute/packet/run.py

Requires Clang, GNU as, XSAVE, OSXSAVE, AVX and XCR0 bits 0..2. Unsupported hosts
fail; there are no feature skips. No privileged instructions or permission changes.
The complete CPUID(D,0).EBX allocation accommodates hosts enabling AVX512/AMX,
but every instruction requests only baseline bits 0..2. No whole-state closure claim.

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

`--seal` and `--check` verify exact names, hashes and ELF identity. Normal execution
removes stale results, rejects Python -O/-OO, bounds each subprocess, requires
exact per-object counters and successful completion of all 18 before writing results.
The Rust test also exercises tamper and optimized-Python rejection. Generating the
packet does not execute it; native execution happens only through `run.py`.
