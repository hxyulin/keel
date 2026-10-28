# Raw GS-base exchange

`x86_swapgs()` takes no arguments and returns nothing. It is append-only
intrinsic ID 818, pinned and classified as a compiler memory barrier. Native
x86 lowering emits one SWAPGS instruction, with no helper call, selector
reload, MSR synthesis or extra hardware fence. Other backends reject it.
There is no optional feature bit. The instruction requires CPL0 in 64-bit mode.

The caller owns the active GS base and IA32_KERNEL_GS_BASE, including their
canonicality, lifetime and every use of either context. SWAPGS exchanges the
two base values; it does not load another GS selector or enter another
privilege level. Both contexts must permit all subsequent compiled code,
stack accesses, ordinary callees and possible asynchronous handlers. Code
that needs GS-relative state must use the intended context at every access.

The API does not infer a user/kernel role from CPL, automatically pair another
exchange, save the previous state, or define a syscall/NMI entry protocol.
Compiler barriers are not a C11 synchronization operation or a speculative
execution barrier. The caller owns event-delivery and speculation policies.
This ordinary-function operation does not implement a system return.

## Validation

The explicit-V1 guest campaign has twelve variants spanning three selection
tiers, both allocators and normal/all-spill register pools. Compiler tests
also cover V2/V3 and text, typed and chunked object equality. Four exchanges
remain in the optimized module, including both exchanges in the round-trip
function. Eight distinct wide SSA inputs remain live across the operations
and ordinary observer calls.

The trusted bridge seeds two mapped GS contexts with distinct data. Generated
code calls an independent assembly observer that reads through GS, while the
bridge separately checks both base MSRs, GS-relative reads at offsets zero
and eight, selector preservation and output canaries. Each variant exercises
both selector zero and selector 16, reverses the mapped contexts, and checks
two single exchanges followed by a round trip. A separate function performs
no GS-relative access and checks two exchanges of wide canonical base values.
The bridge restores the original selector before restoring both base MSRs.
IF remains clear throughout the fixture.

```sh
KEEL_SWAPGS_EXPORT_DIR="$PWD/target/swapgs-exports" \
  cargo test -p keel-codegen --test x86_swapgs
python3 suite/integration/x86-cpl0/descriptor_campaign.py --swapgs \
  --exports target/swapgs-exports --timeout 5
```

All twelve variants and an independent assembly reference pass under QEMU
11.1.2 TCG on macOS ARM64. Six broken controls omit or double the exchange,
clobber a live value, change the selector, truncate a wide base, or do nothing.
All six return the exact READY/FAIL transcript and exit 35, for 19 expected
verdicts. Faults and timeouts never satisfy a negative control.

This is bounded emulated evidence. Native hardware, nested entry, NMI paths,
privilege faults, arbitrary GS use and speculative-execution mitigation are
not qualified. Instruction semantics follow Intel's
[instruction reference, SWAPGS](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf).
