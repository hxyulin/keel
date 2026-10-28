# x86 system instruction support

Keel uses generated ISA forms for system-instruction encoding and decoding.
Public assembly exposure, typed compiler APIs and runtime qualification remain
separate support boundaries.

## Supported areas

- Descriptor-table and selector/status operations.
- Control/debug register access and selected MSR operations.
- Interrupt and halt control, including adjacent STI;HLT.
- Cache/state operations and GS-base exchange.
- Port I/O, processor queries and explicit flag/state APIs.
- An owned IRETQ adapter with a defined GPR/frame image.
- SYSEXIT and RSM assembly forms, without an implied terminal compiler ABI.

See the [ISA overview](userspace-isa-status.md),
[generated coverage](x86-coverage/README.md) and
[transition contracts](x86-system-transitions.md). Individual API references
state their privilege and architectural-state preconditions.

## Structural inventory

Run the read-only inventory against the current generated ledger:

```sh
python3 tools/x86-isa/audit_system.py > target/x86-system-inventory.json
```

The tool joins a manual system roster with imported forms and verifies ledger
input hashes. It reports structural matches, not runtime semantics or a complete
kernel-support verdict. Keep output local rather than committing dated audits.

A complete kernel entry/return, segment-transfer or SMM environment needs an
explicit execution contract. An available opcode or successful emulated witness
does not establish arbitrary hardware or bare-metal qualification.
