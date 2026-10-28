# Documentation

Start with the [toolchain reference](toolchain-overview.md),
[architecture](design.md) and [testing guide](testing.md).

## Supported components

- C preprocessing and native compilation, with selected GNU extensions and
  checked inline assembly.
- Typed IR, optimization levels, per-function policies, native instruction
  selection and register allocation.
- x86-64 and AArch64 assembly/disassembly, with generated ISA metadata.
- Native object emission, archives, ELF and Mach-O linking, and bounded GNU
  linker-script support.
- WebAssembly output within its documented scalar and layout subset.
- LLVM IR import within its documented semantic and ABI subset.
- The pinned minimal Linux kernel's captured 64-bit compile/link/boot path.

These are component summaries, not full language, ISA or platform conformance
claims. The references below define supported subsets and limits. Keep status
here at this level; use crate tests for feature regressions rather than adding
completion ledgers or per-feature progress pages.

## Maintained references

| Area | Reference |
| --- | --- |
| Commands, architecture and tests | [Toolchain](toolchain-overview.md), [design](design.md), [testing](testing.md) |
| C | [Coverage](c-frontend-coverage.md), [extensions](c-extensions.md), [attributes](function-attributes.md), [inline assembly](c-inline-asm.md) |
| Optimization | [Levels](optimization-levels.md), [optimizer contracts](optimizer.md), [SIMD IR](simd-ir.md), [x86 code generation](x86-codegen.md) |
| LLVM import | [Semantics](llvm-semantics.md), [frontend runner](../suite/README.md) |
| Native linking | [ELF](elf-linker.md), [GNU scripts](gnu-linker-scripts.md), [macOS](macos-linker.md), [archives](static-archives.md) |
| Assembly and intrinsics | [x86 assembler](native-x86-assembler.md), [GNU syntax](x86-gnu-assembly-language.md), [intrinsic selection](x86-intrinsics.md), [ISA status](userspace-isa-status.md) |
| Other targets | [ARM Linux](arm-linux.md), [Wasm commands](wasm-cli.md), [Wasm backend](wasm-backend.md) |
| Linux kernel | [Integration](linux-kernel.md), [fixture](../suite/linux-kernel/README.md), [benchmarks](linux-kernel-benchmarks.md) |
| Runtime and debug metadata | [Unwind](unwind-metadata.md), [DWARF](dwarf5.md) |
| Benchmarks and workload validation | [Suite](../suite/README.md), [controlled timing](linux-userspace-timing.md) |

Detailed safety contracts linked from implementation code remain separate where
they define a specific API's preconditions. Generated coverage inventories remain
owned by their generators. Historical progress logs are not maintained references.

## What belongs in Git

Keep documentation that explains current commands, APIs, architecture, semantic
contracts, limitations or reproducible test procedures. Update an existing guide
when behavior changes rather than adding a checkpoint report.

Keep test fixtures, source pins, policies, schemas and generated inventories used
by tests or generators. Keep the reviewed benchmark summaries and chart inputs
that support claims in the project README. A generated file is not necessarily
a disposable result.

Do not commit per-run audit snapshots, logs, raw measurements, object dumps,
progress reports or superseded implementation plans. Put results in ignored
`suite/reports/` or `target/`. Keep large downloaded inputs and build artifacts
there too. Historical reports removed during cleanup remain in Git history;
retaining a historical result in a guide does not qualify the current revision.

Before adding a document, check whether its contract belongs in rustdoc, a crate
README, a test fixture README or one of the references above. Link a new topic
from the guide that owns it.
