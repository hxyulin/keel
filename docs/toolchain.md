# Toolchain reference

Commands, flags and library entry points for the Keel toolchain. For a quick
introduction see the [README](../README.md); for internals see the
[design notes](compiler/design.md). WebAssembly output has its own reference:
[Wasm CLI](targets/wasm/README.md).

## Building

```sh
cargo build --release -p keel            # target/release/keel
cargo install --path crates/keel         # or install it on PATH
cargo build --profile dist -p keel       # fat LTO, ~2-5% faster compiler, slower build
```

Examples below assume `keel` is on `PATH`; `cargo run -p keel -- <args>` works
too.

## Commands

| Command | Input | Output |
| --- | --- | --- |
| `keel cc` | `.c`, `.i` | assembly, object (`--emit-object`), IR (`--emit-ir`), preprocessed text (`-E`) |
| `keel compile` | `.keel` IR | assembly or object |
| `keel opt` | `.keel` IR | optimized IR |
| `keel passes` | | list of optimizer passes |
| `keel asm` | `.s` | object file(s) |
| `keel disasm` | AArch64 or x86-64 Mach-O/ELF | assembly listing |
| `keel archive` | `.o` | static archive |
| `keel link` | `.o`, `.a`, `.so` | executable |
| `keel asm-metrics` | `.s` | instruction metrics JSON |

Run `keel` with no arguments for full usage.

## Compiling C

```sh
keel cc -O hello.c -o hello.s                  # assembly; -g adds debug info
keel cc -O --emit-object hello.c -o hello.o    # object file, no external assembler
keel cc --emit-ir hello.c                      # print Keel IR
keel cc -E -Iinclude -DDEBUG=1 hello.c -o hello.i
keel cc -E -dM hello.c                         # dump the final macro set
keel cc hello.i -o hello.s                     # .i skips preprocessing
```

Keel has its own preprocessor. It derives predefined macros from the target
ABI, answers feature queries itself and embeds its compiler headers
(`stddef.h`, `stdarg.h`, `stdint.h`, `limits.h`, `float.h`, `stdbool.h`,
`stdalign.h`, `stdnoreturn.h`, `stdatomic.h`). It defines `__KEEL__` along
with GCC/Clang compatibility macros such as `__GNUC__` and `__clang__`.
libc headers (`stdio.h`, `stdlib.h`, ...) come from the target SDK or sysroot.

| Flag | Meaning |
| --- | --- |
| `-I dir`, `-D name[=value]`, `-U name` | usual meaning; joined or separate |
| `-isystem dir` | add a system header directory |
| `--sysroot dir`, `-isysroot dir` | target SDK or sysroot |
| `-mmacosx-version-min=X` | macOS deployment target (default 11.0; also `MACOSX_DEPLOYMENT_TARGET`) |
| `--target <triple>` | `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`, `wasm32-unknown-unknown`; default is the host |

Without `--sysroot`, macOS uses `SDKROOT` or the SDK that `xcrun --sdk macosx
--show-sdk-path` prints, as described in [Mach-O linking](linking/macho.md); Linux searches the target multiarch directory and
`/usr/include`. Cross compilation needs a matching sysroot.

## Optimization and code generation

Each stage has its own effort setting:

| Flag | Values | Default | With `-O` |
| --- | --- | --- | --- |
| `--opt-tier` | `none`, `fast`, `basic`, `full` | `none` | `full` |
| `--isel-tier` | `baseline`, `trees`, `full` | `baseline` | `full` |
| `--regalloc` | `fast`, `full` | `full` | `full` |
| `-p` | explicit pass list, e.g. `gvn,dce` | | overrides the IR tier |

- **IR tiers.** `fast` runs local and CFG cleanup plus simple reductions.
  `basic` adds CFG simplification, instruction combining and DCE. `full` runs
  rounds of inlining, GVN, [small-memory expansion](compiler/optimizer.md) and GCM,
  then memory reuse, [stack promotion](compiler/optimizer.md) and bounded loop
  transforms.
- **Instruction selection.** `trees` enables generated tree patterns; `full`
  adds folded operands and register hints. `--isel trees,folds,hints|all|none`
  picks individual passes. The last explicit selector wins over `-O`.
- **Register allocation.** Both allocators are linear scan. `full` keeps
  lifetime holes and register affinities; `fast` uses contiguous intervals.
- **CPU.** `--tune apple-m5` enables Apple M5 scheduling choices (with tree
  selection) without enabling ISA features. On x86-64, `--cpu-level
  v1|v2|v3` sets the ISA level (default `legacy`) and `--target-features`
  enables extras such as AES-NI and PCLMULQDQ.

```sh
keel compile --opt-tier basic --isel-tier trees --regalloc fast input.keel -o out.s
keel cc -O --tune apple-m5 hello.c -o hello.s
keel opt -p sccp,instcombine,dce input.keel
```

Opt-in passes include `sccp`, `if-convert` and `stack-forward-cfg`.
`--verify-each` / `--no-verify-each` control IR verification after every pass
(on by default in debug builds); `--opt-report file.json` writes an
optimization report.

## Targets

**macOS arm64** is the primary target: Mach-O objects, the native linker and
debug info (DWARF, LLDB-tested) are all supported.

**AArch64 Linux** (`--target aarch64-unknown-linux-gnu`) supports AAPCS64,
variadics, atomics, TLS and DWARF, with ELF objects and archives. See
[ARM Linux](targets/aarch64/README.md) for container setup.

**x86-64 Linux** (`--target x86_64-unknown-linux-gnu`) implements the System V
ABI. Pick an explicit `--cpu-level` for deployment. The x86 backend rejects
exception handling and C inline assembly. See [userspace ISA status](targets/README.md).

```sh
keel cc --target x86_64-unknown-linux-gnu --cpu-level v2 --emit-object hello.c -o hello.o
clang hello.o -o hello          # or: keel link --system hello.o -o hello
```

## Assembler

```sh
keel asm crates/keel-asm/examples/return42.s -o return42.o
keel asm --cpu apple-m5 one.s two.s --out-dir build/   # batch: one.o, two.o
keel asm --target x86_64-unknown-linux-gnu input.s -o input.o
keel asm --target x86_64-unknown-linux-gnu --x86-syntax att input.s -o input.o
```

The AArch64 assembler is generated from Arm's machine-readable ISA data and
emits Mach-O or ELF. It covers the integer, floating-point, NEON, LSE atomic
and load/store instructions the compiler uses, plus selected SVE/SME forms
(`--cpu apple-m5` enables M5 extensions). It supports data and TLS sections,
CFI and compact unwind, DWARF line tables, exception tables, `.macro`, `.rept`
and `.if`. Unsupported syntax is an error; this is a subset, not a GNU `as`
replacement. Details: [AArch64 assembler](targets/aarch64/assembler.md),
[x86 assembler](targets/x86_64/assembler.md).

`keel cc` and `keel compile` do not go through assembly text: the backend
builds a typed program that the assembler encodes directly.

## Disassembler

```sh
keel disasm input.o
keel disasm executable -o listing.txt
```

The file selects the architecture. The listing includes addresses, original
bytes and symbol labels. Relocations are not applied or symbolized. AArch64
uses `.inst` for unsupported formatting. x86 stops at an unknown instruction
and prints the rest of that section as bytes. See the
[API and limits](../crates/keel-disasm/README.md).

## Linking and archives

```sh
keel link hello.o -o hello                            # macOS arm64, uses the SDK
keel link --target x86_64-unknown-linux-gnu start.o lib.a -o program
keel link --system hello.o -o hello                   # delegate to the system driver
keel archive -o libfoo.a foo.o bar.o
```

- The [macOS linker](linking/macho.md) links arm64 executables against the
  SDK's `.tbd` interfaces and ad-hoc signs the output. Supports
  `--dead-strip`, `-L`/`-l`, `--rpath`, `--debug-map` and `--dsym`.
- The [ELF linker](linking/elf.md) links fixed-address executables entered at
  `_start`, optionally against shared libraries. You supply startup code;
  for libc/CRT startup use `--system`.
- [Archives](linking/archives.md) are deterministic and indexed (GNU format
  for ELF, BSD format for Darwin).

## Library API

Each stage is a crate that can be used directly:

```rust,ignore
let program = keel_codegen::compile_program(&module, keel_codegen::Target::Aarch64AppleDarwin)?;
let object = keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())?;
```

- `keel_c::preprocess` / `keel_c::compile`: C to IR.
- `keel_opt::Pass` and `run_pipeline`: custom optimizer pipelines.
- `CodegenPipeline`: plug in an `InstructionSelector` or `RegisterAllocator`.
- `keel_asm::x86::assemble_program_object`: x86-64 ELF objects.

## Profiling

- Build with `--features profile-with-tracy` and attach
  [Tracy](https://github.com/wolfpld/tracy) to profile passes, selection,
  allocation and lowering. Instrumentation is compiled out otherwise.
- `keel asm-metrics input.s` and
  [`tools/instruction_metrics.py`](../tools/instruction_metrics.py) report
  instruction sizes and modeled costs; see [instruction costs](compiler/optimizer.md).

## Language support at a glance

The C frontend targets C11 with common GNU extensions. Highlights, each with
its own reference:

- [GNU vectors and integer modes](frontends/c/extensions.md), VLAs and complex
  numbers, K&R definitions,
  `__attribute__((cleanup))`
- [Overflow builtins](frontends/c/extensions.md) up to 128 bits,
  [memory builtins](compiler/optimizer.md), atomics including floating-point
- [Function attributes](compiler/function-attributes.md), [weak symbols](frontends/c/extensions.md),
  [constructors/destructors](frontends/c/extensions.md), [sections](frontends/c/extensions.md)
- [AArch64 inline assembly](targets/aarch64/c-inline-asm.md) (checked subset)
- [x86 intrinsics](targets/x86_64/intrinsics.md) and [SIMD](targets/README.md):
  NEON, SSE, AVX/AVX2, FMA, F16C (no AVX-512)

Many attributes are parsed but ignored; unsupported ones are diagnosed. Keel
does not claim full GCC or Clang compatibility. See
[C frontend coverage](frontends/c/README.md) for the full list.

The [LLVM IR importer](../suite/README.md) and
[Cranelift bridge](../crates/keel-cranelift/README.md) are diagnostic tools,
not alternative production paths.

## Development

```sh
python3 tools/test.py codegen --quick                    # Core backend regressions
python3 tools/test.py codegen --test stack_frame --quick # Changed feature target
python3 tools/test.py assembler --quick                  # Assembler component
cargo suite test lua                                    # Explicit application check
```

Use `python3 tools/test.py full` for merge-boundary workspace verification.
Unfiltered codegen runs include broad qualification matrices; they are not the
normal development check. See [test selection](development/testing.md).

End-to-end tests need `clang` (and `clang++` for C++ drivers); ARM64 debug
tests need `lldb`. Tests run on macOS arm64 and x86-64 Linux.

| Tests | Location | Format |
| --- | --- | --- |
| IR golden | `crates/keel-ir/tests/golden/` | `BLESS=1 cargo test` rewrites `.out` files |
| Optimizer passes | `crates/keel-opt/tests/golden/` | first line `; passes: a, b` |
| Codegen programs | `crates/keel-codegen/tests/programs/` | `; expect: exit N` or `; expect: trap`; optional same-named `.c` driver; built in 8 allocator/selection configurations |
| C programs | `crates/keel-c/tests/programs/` | output must match Clang, with and without `-O` |
| Applications | `suite/c/` | see the [suite guide](../suite/README.md) |

Commits follow [Conventional Commits](https://www.conventionalcommits.org/),
scoped by area: `feat(ir): ...`, `fix(codegen): ...`, `docs: ...`.
