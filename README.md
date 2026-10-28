# Keel

A C compiler toolchain written in Rust: preprocessor, frontend, SSA optimizer,
native backend, assembler, disassembler, archiver and linker. Compiling C to an
object file never invokes Clang.

<p align="center"><img src="docs/assets/readme-pipeline.svg" alt="Keel toolchain stages: C frontend, optimizer, backend, assembler and linker, and the keel commands that run each span" width="720"></p>

Every stage is also a standalone command, so you can start or stop anywhere in
the pipeline.

Optimization presets `-O0`, `-O1`, `-O2`, `-O3` and `-Os` select distinct
compiler policies. See [optimization levels and measurements](docs/compiler/optimization-levels.md)
for stage overrides, size-policy limits and x86-64 code models.

## Install

```sh
cargo install --path crates/keel
```

Requires a Rust toolchain with edition 2024 support.

## Usage

Compile and link a C program (macOS arm64):

```sh
keel cc -O --emit-object hello.c -o hello.o
keel link hello.o -o hello
./hello
```

Run any stage on its own:

```sh
keel cc --emit-ir hello.c -o hello.keel        # C → Keel IR
keel opt -p gvn,dce hello.keel                 # run optimizer passes
keel compile -O --emit-object hello.keel -o hello.o
keel asm input.s -o input.o                    # standalone assembler
keel disasm hello.o                           # AArch64 or x86-64 disassembly
keel archive -o libinput.a input.o             # static archive
```

Omit `--emit-object` to get assembly text. Use `--target` to cross-compile,
e.g. `--target x86_64-unknown-linux-gnu`; hosted programs still need that
platform's sysroot (`--sysroot`).

Compile freestanding C to WebAssembly:

```sh
keel cc --target wasm32-unknown-unknown --emit-wasm --export add add.c -o add.wasm
```

## Platforms

| | macOS arm64 | Linux AArch64 | Linux x86-64 | wasm32 |
| --- | :---: | :---: | :---: | :---: |
| C compiler | ✓ | ✓ | ✓ | integer/pointer subset |
| Output | Mach-O | ELF | ELF | `.wasm` module |
| Assembler | ✓ | ✓ | ✓ (Intel, AT&T) | — |
| Disassembler | ✓ | ✓ | ✓ | — |
| Linker | ✓ | ✓¹ | ✓¹ | — |

¹ Native ELF links need caller-supplied startup code; `keel link --system`
links hosted programs through the system driver.

C support targets C11 with common GNU extensions (vectors, VLAs, attributes,
complex numbers, AArch64 inline assembly). See [C coverage](docs/frontends/c/README.md)
and [ISA status](docs/targets/README.md) for the details and limits.

## Linux kernel milestone

Keel compiles and assembles the core of a pinned minimal x86-64 Linux v6.12
kernel. The resulting kernel boots and passes checked filesystem, memory,
fork/COW and pipe workloads. No replacement GCC C objects or manual IR patches
are needed. Keel also performs [native core linking](docs/integrations/linux-kernel.md),
including GNU script layout, regular/thin archives and relocatable aggregation.
The build uses captured preprocessed inputs and reference boot, decompressor
and vDSO stages. Full raw-source Kbuild remains work in progress.
[Build and boot evidence](docs/integrations/linux-kernel.md).

The earlier GNU-linked benchmark image is **1.61 MB versus GCC's 0.93 MB**, 73% larger. Guest
init-entry timing is similar, about 257 ms versus 259 ms. Five checked
kernel-heavy workloads take **1.52–2.38× as long** as GCC 14.4's size-optimized
kernel. These measurements use nine alternating pairs on an AMD Ryzen 9
9950X3D with native KVM execution, one vCPU and identical userspace.
[Methodology, samples and limits](docs/benchmarks/linux-kernel.md).

<p align="center"><img src="docs/assets/readme-kernel-runtime.svg" alt="Pinned Linux v6.12 kernel workload time, Keel relative to GCC 14.4 -Os: getpid 1.52x, open/read/close 2.38x, page faults and protection 2.17x, fork/COW/wait 2.06x, pipe ping-pong 1.74x. Native KVM, one vCPU, median of nine paired runs; lower is better." width="720"></p>

## Performance

Keel compiles quickly and supports application workloads and the kernel
milestone above. Generated code still trails GCC and Clang in runtime and
size optimization on the measured workloads.
The application and toolchain measurements below use an Apple M5 Max.
[Compiler](docs/benchmarks/compiler-20261024.md) ·
[assembler/linker](docs/benchmarks/assembly-link-20261024.md) ·
[runtime](docs/benchmarks/runtime-20261022.md) methodology

<p align="center"><img src="docs/assets/readme-c-object.svg" alt="C source to object file: Keel 4.32 ms, Keel -O 5.35 ms, Clang -O0 16.8 ms, Clang -O2 41.8 ms" width="720"></p>

<p align="center"><img src="docs/assets/readme-runtime.svg" alt="Run time of Keel -O programs relative to Clang -O2: Lua 1.34x, SQLite 1.48x, LZ4 1.49x, yyjson 1.66x, QuickJS 1.89x; 1.6x slower overall" width="720"></p>

<p align="center"><img src="docs/assets/readme-ir-object.svg" alt="Keel IR to object file, backend only: Keel baseline 377 µs, Keel full 413 µs, Cranelift none 775 µs, Cranelift speed 1011 µs" width="720"></p>

<p align="center"><img src="docs/assets/readme-assembly.svg" alt="Assembly to object file: keel asm 5.60 ms, clang -cc1as 16.8 ms, clang -c 23.1 ms" width="720"></p>

<p align="center"><img src="docs/assets/readme-link.svg" alt="Objects to signed executable: keel link 8.45 ms, Apple ld 14.8 ms, clang 22.0 ms" width="720"></p>

Charts are generated from the benchmark data by
[`docs/assets/readme-render-charts.py`](docs/assets/readme-render-charts.py).

## Documentation

- [Documentation index and retention policy](docs/README.md)
- [Toolchain reference](docs/toolchain.md): commands, flags, optimization tiers
- [Design](docs/compiler/design.md): IR, optimizer and backend architecture
- [C frontend](docs/frontends/c/README.md) · [SIMD IR](docs/compiler/simd-ir.md)
- [macOS linker](docs/linking/macho.md) · [ELF linker](docs/linking/elf.md) · [Static archives](docs/linking/archives.md)
- [x86 assembler](docs/targets/x86_64/assembler.md) · [ARM Linux](docs/targets/aarch64/README.md)
- [Disassembler](crates/keel-disasm/README.md) · [Shared x86 tables](tools/x86-isa/README.md)
- [WebAssembly](docs/targets/wasm/README.md)
- [Benchmark and application suite](suite/README.md)

## Development

```sh
cargo build
python3 tools/test.py codegen --quick  # Example: core codegen regressions
```

Select the component and feature target you changed. Reserve full workspace
verification for integration boundaries. See [targeted and full tests](docs/development/testing.md)
for selections, qualification coverage and the optimized test profile.

End-to-end tests need `clang` (and `clang++` for C++ drivers). See the
[development section](docs/toolchain.md#development) for test layout.
Commits follow [Conventional Commits](https://www.conventionalcommits.org/).
