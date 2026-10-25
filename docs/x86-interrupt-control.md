# Interrupt control and halt

`x86_cli()`, `x86_sti()` and `x86_hlt()` expose the corresponding raw long-mode
instructions. `x86_sti_hlt()` emits adjacent STI and HLT instructions as one
IR operation. All take no arguments, return nothing, are pinned compiler
memory barriers, and require the x86 backend. IDs 814 through 817 are
append-only. They require no optional CPU feature bit.

The caller owns hardware permission and the asynchronous execution context.
CLI/STI require the applicable CPL/IOPL permission; HLT requires CPL0.
Virtualization interception and hardware faults remain outside the compiler.
CLI changes maskable-interrupt acceptance on the current logical processor.
It does not exclude every exception, NMI or SMI. STI can allow event delivery
inside the current compiled function. These operations neither save nor
restore the previous IF state and do not provide a C11 synchronization operation
or an extra hardware memory fence.

A caller enabling interrupts must provide valid event delivery, mappings,
stacks and handlers that preserve the interrupted ABI and every live machine
state component. Ordinary compiler function entry is not an interrupt-entry
ABI. The compiler does not synthesize an IDT, TSS, register-save wrapper,
SIMD save or red-zone protection. Those remain the caller's responsibility.

HLT is a resumable wait, not an IR terminator. If execution resumes normally,
compiled instructions after the intrinsic continue. An appropriate wake event
must exist; a wait can otherwise remain halted indefinitely. Reset or other
state transitions do not imply an ordinary function return. A single HLT does
not implement an interrupt-enable/wait protocol.

The combined operation guarantees adjacent STI;HLT bytes, with no allocation,
spill, label or helper call between them. With IF clear on entry and a valid
permitted STI, the architectural interrupt shadow covers HLT. If IF is already
set, STI does not establish the same shadow guarantee. Separate STI and HLT
IR operations carry no adjacency promise. The combined operation provides no
general exclusion guarantee for NMI, SMI, debug or other exceptional events.

## Disposable IRQ validation

The fixture owns its fresh VM's GDT, TSS, IDT, PIC and PIT. It installs one IRQ0
interrupt gate using a private IST stack, masks other PIC sources, and uses
a one-shot PIT interval of about 55 ms. It resets the PIC pending state on each
arm. The handler checks that its active stack is inside the private IST area and
that the interrupted stack is outside it. It saves/restores every GPR and uses
IRETQ; the compiled fixture
uses only scalar GPR values. This does not qualify FP/SIMD handler state.
Cleanup leaves IF clear and the PIC masked for the harness's final debug exit.
It does not restore a general pre-existing operating system environment.

The CLI/STI cases independently check IF with trusted PUSHFQ observations.
Both wait cases check exactly one timer IRQ, saved IF set, handler IF clear,
eight wide SSA arguments and output canaries. For the combined operation, the
fixture polls until IRQ0 is pending while IF remains clear before calling the
compiled function. A bounded poll or VM timeout is a failure, never success.

The IRQ handler records the hardware interrupt-frame RIP without modifying
that frame. After all guest checks succeed, the trusted bridge prints two
fixed-format RIP records. The campaign independently runs GNU objdump over
each linked ELF function. It requires the recorded RIP to equal the unique
one-byte HLT's fall-through address. The combined function must also have an
immediately preceding one-byte STI. Checking an IRQ count or the previous
byte alone would not establish execution of a HLT instruction.

```sh
KEEL_INTERRUPT_EXPORT_DIR="$PWD/target/interrupt-exports" \
  cargo test -p keel-codegen --test x86_interrupt_control
python3 suite/x86-cpl0/descriptor_campaign.py --interrupt-control \
  --exports target/interrupt-exports --objdump x86_64-elf-objdump --timeout 5
```

Use the runner's assembler, linker, objcopy and QEMU overrides when necessary.
All twelve explicit-V1 variants and an independent assembly reference pass
under QEMU 11.1.2 TCG on macOS ARM64. Five broken controls omit CLI, STI or HLT,
clobber a live value, or return without operating. All five returned the exact
READY/FAIL transcript and exit 35 in this run, for 18 expected verdicts.

For this mode, a normally returned complete report with incorrect HLT evidence
also counts as rejection of a broken control. This matters if an omitted HLT
races the timer. Malformed output, faults, tool errors and timeouts cannot
satisfy a negative control. Other campaign modes retain their exact transcript
and exit requirements. Five evidence unit tests cover strict reports, decoded
boundaries, adjacency, mismatched RIPs and rejection rules.

Compiler checks cover V1/V2/V3, three selection tiers, both allocators and
normal/all-spill pools. They retain all operations after optimization, require
STI/HLT adjacency and compare text, typed and chunked object routes. Decoder
witnesses check the individual instruction bytes and truncation.

This is bounded emulated IRQ evidence. Native hardware, arbitrary event
sources, exceptional wakeups, concurrent CPUs, privilege faults and a general
interrupt handler ABI remain unqualified. Instruction semantics follow Intel's
[instruction reference A-L](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf)
and [instruction reference M-U](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf).
The PIT interval uses QEMU's [8254 clock frequency](https://raw.githubusercontent.com/qemu/qemu/v11.1.2/include/hw/timer/i8254.h).

The full workspace run passes with this implementation: 2,666 tests pass,
none fail and 13 are ignored across 485 reported groups. Fresh exports pass
all 155 expected verdicts across nine CPL0 campaigns. Both generator freshness
checks, 25 generator/pilot tests and all 15 runner/evidence unit tests pass.
This full run precedes the later register-schema import.
