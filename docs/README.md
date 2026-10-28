# Documentation

Start with the [toolchain reference](toolchain.md),
[architecture](compiler/design.md) and [testing guide](development/testing.md).

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
| Commands, architecture and tests | [Toolchain](toolchain.md), [design](compiler/design.md), [testing](development/testing.md) |
| C | [Coverage](frontends/c/README.md), [extensions](frontends/c/extensions.md), [attributes](compiler/function-attributes.md), [inline assembly](frontends/c/inline-asm.md) |
| Optimization | [Levels](compiler/optimization-levels.md), [optimizer contracts](compiler/optimizer.md), [SIMD IR](compiler/simd-ir.md), [x86 code generation](targets/x86_64/codegen.md) |
| LLVM import | [Semantics](frontends/llvm.md), [frontend runner](../suite/README.md) |
| Native linking | [ELF](linking/elf.md), [GNU scripts](linking/gnu-scripts.md), [macOS](linking/macho.md), [archives](linking/archives.md) |
| Assembly and intrinsics | [x86 assembler](targets/x86_64/assembler.md), [GNU syntax](targets/x86_64/gnu-assembly-language.md), [intrinsic selection](targets/x86_64/intrinsics.md), [ISA status](targets/README.md) |
| Target entry points | [All targets](targets/README.md), [x86-64](targets/x86_64/README.md), [AArch64](targets/aarch64/README.md), [WebAssembly](targets/wasm/README.md) |
| Linux kernel | [Integration](integrations/linux-kernel.md), [fixture](../suite/integration/linux-kernel/README.md), [benchmarks](benchmarks/linux-kernel.md) |
| Runtime and debug metadata | [Unwind](linking/unwind.md), [DWARF](linking/dwarf5.md) |
| Benchmarks and workload validation | [Suite](../suite/README.md), [controlled timing](benchmarks/linux-timing.md) |

Detailed safety contracts linked from implementation code remain separate where
they define a specific API's preconditions. Generated coverage inventories remain
owned by their generators. Historical progress logs are not maintained references.

## Where to put a change

- `compiler/`: shared IR, optimization and function policies.
- `frontends/`: C and LLVM input semantics.
- `linking/`: objects, archives, linking and runtime/debug metadata.
- `targets/`: target-specific assembly, lowering and machine-state contracts.
- `integrations/`: complete workloads such as the pinned Linux kernel.
- `development/`: contributor workflows and testing.
- `benchmarks/`: reviewed methodology and public measurement summaries.

Keep target-specific details under their target entry point. Update the owning
reference and navigation when moving or adding a topic; do not add a parallel
flat document or a progress log.

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
