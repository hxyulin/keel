# Owned IRETQ transport

`keel_codegen::x86_64::iretq` exposes a linkable ELF adapter through
`program()` and equivalent Intel-syntax `assembly()`. Import `declaration()`
(or its signature and `noreturn` attribute) and link the adapter once. The
symbol is `__keel_x86_iretq`. Its SysV AMD64 C argument is one image pointer
in RDI; it returns no values and never returns to the compiled caller.

```text
decl @__keel_x86_iretq(ptr) noreturn
func @resume(ptr) noreturn {
entry(v0:ptr):
 call @__keel_x86_iretq(v0)
 trap
}
```

Keep the ordinary impure call contract and explicitly terminate its IR path.
Do not mark the declaration pure, readonly, returns-twice or `cc(fast)`.
The caller's ordinary stack frame is abandoned. This is a terminal assembly
transport, not a returning intrinsic or a naked compiled function.

## Image and access contract

`iretq::Image` is 160 bytes aligned to eight bytes. Each target field occupies one
little-endian 64-bit word. A remote host must encode words in that byte order.
Its field order and offsets are public:

| Offsets | Words |
|---|---|
| 0, 8, 16, 24 | RAX, RCX, RDX, RBX |
| 32, 40, 48 | RBP, RSI, RDI |
| 56, 64, 72, 80 | R8, R9, R10, R11 |
| 88, 96, 104, 112 | R12, R13, R14, R15 |
| 120, 128, 136, 144, 152 | RIP, CS, RFLAGS, RSP, SS |

The adapter reads all fifteen non-RSP GPR words. R10 holds the image pointer
until its final load. The final three instructions set RSP to image + 120,
restore R10 from image + 72, and execute IRETQ. The processor consumes the
five-word frame, including for a same-CPL 64-bit return. The transport emits
no memory stores, helper calls, spills or ordinary epilogue after installing
the target state. It provides no unwind metadata or exception recovery.

The caller exclusively owns the image and keeps its bytes and code mapped
through the transfer. At its final instructions the image is an actual stack.
Asynchronous events and faults may write below its frame. Provide writable
backing and headroom with valid handlers, or independent IST stacks and a
compatible event policy. Disable maskable interrupts before calling; NMI,
debug events and faults remain caller-owned. IRETQ can unblock NMIs even when
it faults. The absence of stores in this transport does not make its call
readonly or make an arbitrary buffer a valid event stack.

## Architectural preconditions

Execute at CPL0 in 64-bit mode with live NT clear and CET shadow stacks
disabled. The target is valid 64-bit code at the same or a less privileged
level. The caller supplies valid selectors and descriptor tables, canonical
RIP/RSP, mappings and legal flags. Selector words are zero-extended.
Target code must accept the restored registers and stack, including its ABI
alignment and return-address policy. The adapter does not validate addresses,
reload unrelated machine state, sanitize flags, create a usable exception
stack or install an interrupt-entry protocol.
It leaves IRETQ's architectural effects on segment selectors and flags intact.

The image is a GPR and return-frame contract. SIMD/x87 state, FS/GS bases,
address-space state, debug registers and other context components need separate
ownership. This API is not a complete context switch. Compatibility/legacy
mode returns, task returns, CET, VMX/SVM and FRED protocols are outside this
transport. Semantics follow Intel's [IRETQ instruction reference](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf)
and [64-bit interrupt frame rules](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-3a-part-1-manual.pdf).

## Validation

Compiler tests cover V1/V2/V3, all three selection tiers, both allocators and
normal/all-spill pools. They check explicit terminal caller paths and
text/typed/chunked object equality. Typed adapter output contains no text
fallback; its 68 instruction bytes match GNU as and Clang independently.

The explicit-V1 guest campaign has twelve generated caller variants and an
independent GNU-assembled reference. Each first restores all fifteen GPRs
onto a distinct CPL0 stack with RFLAGS 2, then restores them onto another stack
at CPL3 with RFLAGS 0x43. That second RIP and RSP use a high canonical alias,
which distinguishes full-width addresses from truncation. The landing shim
captures registers, RSP, flags and selectors before using them. A private DPL3 interrupt gate and TSS return
control to the trusted CPL0 verifier. That verifier separately checks the
processor's saved CS, SS, RSP and continuation RIP, the caller's eight wide
values, output canaries and image preservation.

The fixture owns a private GDT/TSS/IDT. It temporarily grants user access to
the first 2 MiB identity mapping and a high canonical alias. It restores the
original page entries/CR3 before exit. This observes privilege transitions; it does not test memory
isolation. Private descriptor tables remain installed until disposable guest
exit. Both landings keep IF clear, and no asynchronous event is injected.

```sh
KEEL_IRETQ_EXPORT_DIR="$PWD/target/iretq-exports" \
  cargo test -p keel-codegen --test x86_iretq
python3 suite/integration/x86-cpl0/descriptor_campaign.py --iretq \
  --exports target/iretq-exports --timeout 5
```

All thirteen positive variants pass under QEMU 11.1.2 TCG on macOS ARM64.
Seven broken controls omit R10 restoration, use RET, change flags, overwrite
an image guard, corrupt a live value, skip the CPL3 transition or return without
observing anything. Each returns exact READY/FAIL with exit 35, for twenty
expected verdicts. Faults and timeouts never satisfy a negative control.
Native hardware, malformed frames, exception recovery, asynchronous delivery,
noncanonical targets and other descriptor/flag combinations remain unqualified.
