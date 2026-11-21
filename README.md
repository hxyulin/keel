# Keel

A C compiler toolchain written in Rust: preprocessor, frontend, SSA optimizer,
native backend, assembler, disassembler, archiver and linker. Compiling C to an
object file never invokes Clang.

<p align="center"><img src="docs/assets/readme-pipeline.svg" alt="Keel toolchain stages: C frontend, optimizer, backend, assembler and linker, and the keel commands that run each span" width="720"></p>

Every stage is also a standalone command, so you can start or stop anywhere in
the pipeline. The experimental [C API](crates/keel-ffi/README.md) embeds IR
construction, optimization and native object emission in other languages.

Optimization presets `-O0`, `-O1`, `-O2`, `-O3` and `-Os` select distinct
compiler policies. See [optimization levels](docs/compiler/optimization-levels.md)
for stage overrides and size-policy limits, and
[x86 code generation](docs/targets/x86_64/codegen.md#code-models) for x86-64
code models.

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
complex numbers, AArch64 and x86-64 inline assembly). See [C coverage](docs/frontends/c/README.md)
and [ISA status](docs/targets/README.md) for the details and limits.

## Linux kernel milestone

Keel compiles the core of a pinned minimal x86-64 Linux v6.12 kernel that boots
and passes filesystem, memory, fork/COW and pipe tests.
[Build and boot evidence](docs/integrations/linux-kernel.md).

## Performance

In the [November 21 checkpoint](docs/benchmarks/checkpoint-20261121.md),
applications built with Keel `-O` took **1.11×** Clang `-O2` wall time on
Apple M5 Max and **1.13×** on AMD Zen 5 Linux, by geometric mean across six
applications. QuickJS string remains the largest runtime gap on both hosts.

<p align="center"><img src="docs/assets/readme-runtime.svg" alt="Keel -O application wall time relative to Clang -O2 on Apple M5 Max. Geometric mean 1.11×. QuickJS string is the slowest individual workload at 1.26×." width="720"></p>

<p align="center"><img src="docs/assets/readme-runtime-x86.svg" alt="Keel -O application wall time relative to Clang -O2 on Zen 5 Linux. Geometric mean 1.13×. LZ4 runs at 0.91×, and QuickJS string is the slowest individual workload at 1.35×." width="720"></p>

The compiler charts use one C kernel translation unit. The backend and
Cranelift runtime comparisons feed the same optimized Keel IR to each backend.

<p align="center"><img src="docs/assets/readme-c-object.svg" alt="Linux C to object time. Keel -O takes 7.77 ms and Clang -O2 takes 85.5 ms." width="720"></p>

<p align="center"><img src="docs/assets/readme-ir-object.svg" alt="Linux backend time on the same optimized Keel IR. Keel full takes 1.53 ms and Cranelift speed takes 2.48 ms." width="720"></p>

<p align="center"><img src="docs/assets/readme-cranelift-runtime.svg" alt="Geometric mean across 21 kernels from the same optimized Keel IR. Keel takes 0.96× Cranelift speed time on x86-64 Linux and 0.90× on AArch64 macOS." width="720"></p>

The linker benchmarks use identical input objects for every linker on each
platform and check the resulting executables against an independent oracle.

<p align="center"><img src="docs/assets/readme-link-elf.svg" alt="Linux ELF link time. Keel link takes 2.97 ms, GNU ld.bfd takes 6.00 ms, and LLVM ld.lld takes 6.60 ms." width="720"></p>

<p align="center"><img src="docs/assets/readme-link.svg" alt="Signed Mach-O link time. Keel link takes 4.82 ms, Apple ld takes 15.3 ms, and the Clang driver takes 23.6 ms." width="720"></p>

<p align="center"><img src="docs/assets/readme-assembly.svg" alt="Mach-O assembly time for identical input text. Keel asm takes 7.18 ms, clang -cc1as takes 19.6 ms, and clang -c takes 26.3 ms." width="720"></p>

Charts are generated from the benchmark data by
[`docs/assets/readme-render-charts.py`](docs/assets/readme-render-charts.py).
`python3 tools/test.py charts` checks them against that data.

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
[test layout](docs/development/testing.md#test-layout).
Commits follow [Conventional Commits](https://www.conventionalcommits.org/).

## License

Keel is licensed under either the [MIT License](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option.
Third-party material retains its own license notices and terms.
