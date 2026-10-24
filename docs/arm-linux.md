# AArch64 Linux toolchain

Keel supports **64-bit, little-endian AArch64 Linux** as
`--target aarch64-unknown-linux-gnu` (alias `aarch64-linux-gnu`). On an ARM Linux
host, `cc`, `compile`, `opt`, `asm`, `archive` and `link` select it by default.

| Capability | Status |
|---|---|
| C and IR compilation, assembly, object emission | In process, no external tools |
| Freestanding static executables | Native Keel linker |
| Hosted executables (libc, CRT, shared libraries) | `link --system` (invokes Clang) |
| ABI | Base AAPCS64, including variadics and initial-exec TLS |
| Debug info | CFI and DWARF line/type/variable info in assembly and objects |
| Exception handling, `long double` arithmetic, SVE operations | Diagnosed as unsupported |

## ARM development container

From the repository root:

```sh
docker build --platform linux/arm64 -t keel-arm-linux -f tools/arm-linux/Dockerfile .
docker run --rm -it --platform linux/arm64 -v "$PWD:/workspace" keel-arm-linux
```

The Ubuntu 24.04 image installs native Clang, compiler-rt, LLVM tools, GNU
binutils, LLD, build tools and stable Rust. `CARGO_TARGET_DIR=/tmp/keel-target`
separates Linux artifacts from the mounted host's `target/`. Inside:

```sh
cargo build -p keel
KEEL=/tmp/keel-target/debug/keel
cat > /tmp/hello.c <<'C'
#include <stdio.h>
int main(void) { printf("hello ARM %d\n", 42); return 0; }
C
"$KEEL" cc -O --emit-object /tmp/hello.c -o /tmp/hello.o
"$KEEL" link --system /tmp/hello.o -o /tmp/hello
/tmp/hello
```

For assembly output, omit `--emit-object`, write `hello.s`, and assemble it with
`keel asm` or native Clang. `keel compile` also accepts textual IR and emits
assembly or ELF objects. Outside Linux ARM, pass the target explicitly; C
sources that include system headers need a matching `--sysroot`. Native Linux
preprocessing searches `/usr/include/aarch64-linux-gnu` and `/usr/include`.

## Using only Keel

Once the Rust CLI has been built, freestanding programs can use Keel for every
C/IR compilation, assembly, archive and link step. No GCC, GNU ld, Clang or LLVM
executable is required by this pipeline. For example, with `KEEL` set to the
absolute path of the built CLI:

```sh
cat > /tmp/answer.c <<'C'
int answer(void) { return 42; }
C
cat > /tmp/start.s <<'ASM'
.text
.global _start
.type _start, %function
_start:
    bl answer
    mov x8, #93
    svc #0
.size _start, .-_start
ASM
# Empty PATH verifies these steps cannot discover an external toolchain.
PATH= "$KEEL" cc --target aarch64-linux-gnu -O /tmp/answer.c -o /tmp/answer.s
PATH= "$KEEL" asm --target aarch64-linux-gnu /tmp/answer.s -o /tmp/answer.o
PATH= "$KEEL" asm --target aarch64-linux-gnu /tmp/start.s -o /tmp/start.o
PATH= "$KEEL" archive --target aarch64-linux-gnu /tmp/answer.o -o /tmp/libanswer.a
PATH= "$KEEL" link --target aarch64-linux-gnu /tmp/start.o -L /tmp -lanswer -o /tmp/answer
/tmp/answer; echo $? # 42 on ARM Linux
```

`cc --emit-object` skips the textual assembly round trip. `compile` provides
the same routes for textual IR. The executable above uses Linux's exit syscall
and supplies its own entry point. Hosted C programs still need libc, CRT startup
and system linking; operations that emit runtime helper calls (for example wide
integer division or memory routines) also need implementations of those helpers.
Building Keel itself uses Rust and its configured host linker.

The `standalone_c_ir_assembly_archive_and_link` CLI test covers C preprocessing,
C and IR compilation, both object routes, archives and native linking with an
empty `PATH`. It executes on ARM Linux. On other Linux architectures, set
`KEEL_AARCH64_RUNNER` to an absolute `qemu-aarch64` or `qemu-aarch64-static` path
to execute the resulting static ARM binaries too:

```sh
KEEL_AARCH64_RUNNER=/absolute/path/to/qemu-aarch64-static \
  cargo test -p keel --test aarch64_elf
```

QEMU is only the execution environment; it does not compile, assemble or link.

## ABI and code generation

The Linux implementation follows the base
[Arm AAPCS64](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst).
It shares instruction selection and allocation with Darwin ARM64 while selecting
Linux symbols, ELF sections, argument placement and variadic state:

- Integers/pointers use x0–x7 and floats/SIMD use v0–v7. Stack arguments occupy
  at least eight bytes. Naturally 16-byte aligned integer/aggregate arguments
  round the general register cursor to an even register. Aggregate top-level
  over-alignment is distinguished from member alignment.
- Small structs and HFAs use registers; large aggregates use caller copies and
  indirect results. Register exhaustion, stack placement and HFA returns follow
  AAPCS64 rather than Darwin's compact stack rules.
- `va_list` is a 32-byte struct with stack, GP/FP save-area tops and signed GP/FP
  offsets. Variadic functions preserve the argument registers. `va_start`,
  scalar `va_arg` (including i128), `va_copy`, passed `va_list` objects and libc
  variadic interfaces are supported. Variadic aggregate `va_arg` remains outside
  the frontend's supported scope.
- Plain `char` and `wchar_t` are unsigned. Target macros describe Linux LP64 and
  binary128 `long double` storage. As on the existing Linux x86 target, long
  double values/arithmetic are diagnosed as unsupported; storage/type
  declarations remain available. SVE builtin types are declaration-only opaque
  types for libc headers; scalable-vector operations are unsupported.
- Baseline ARMv8.0 atomics use LL/SC loops instead of requiring LSE. The Linux
  target currently uses conservative acquire/release ordering for these loops.
  TLS uses the initial-exec model and TPIDR_EL0; independent threads get distinct
  storage. Initial-exec TLS in dynamically loaded modules has the normal static
  TLS allocation restrictions.

Both assembly and direct-object output support CFI, DWARF line/type/variable
information, initialized data, BSS and TLS data. ELF metadata/layout/DWARF code
is shared with the x86 ELF writer. ARM ELF TLS variables currently have an
unavailable DWARF location, avoiding an invalid dynamic relocation in debug
data; stepping and ordinary variable locations remain available. Exception
handling on this target is explicitly diagnosed. Existing frontend restrictions
(such as weak attributes) still apply; this is a new target for Keel's supported
C/IR language, not a claim of complete ISO C or Clang compatibility.

## Assembly, archives and native linking

The assembler uses the existing A64 encoder with GNU ELF names and relocation
syntax, without Darwin underscore decoration. It accepts named labels,
global/weak/hidden symbols, function/object types and sizes, common definitions,
ELF sections, scalar/string data, alignment, CFI and debug directives. It shares
ELF metadata support with the x86 assembler. Numeric local labels, assembly
macros and architecture-selection directives are not implemented on the ARM
source path; unsupported syntax is diagnosed.

The native linker produces fixed-address ET_EXEC executables entered at
`_start`, or a symbol selected with `-e`. It shares symbol resolution,
weak/common definitions, regular archive extraction, BSS, unwind record merging
and `.eh_frame_hdr` generation with the ELF linker. Load segments use 64 KiB
alignment for Linux's 4/16/64 KiB page sizes, with a non-executable GNU stack.
Supported relocations include ABS64/32, PREL64/32, CALL26/JUMP26, conditional/test
branches, literal loads, ADR, ADRP, ADD low-12, load/store low-12 and GOT page/load
references. Overflow, misalignment and unsupported relocations are diagnosed;
branch veneers are not generated.

Native linking requires startup code. Shared libraries, automatic libc/CRT
startup, TLS allocation, GNU properties and runtime initialization arrays remain
outside its supported scope. Use `link --system` for a normal hosted executable.
Native linker debug inputs are omitted with warnings; use system linking when
retaining full executable debug information.

A freestanding assembly/archive example inside the container:

```sh
KEEL=/tmp/keel-target/debug/keel
FIXTURE=crates/keel-link/tests/fixtures/aarch64
"$KEEL" asm "$FIXTURE/start.s" -o /tmp/start.o
"$KEEL" asm "$FIXTURE/helper.s" -o /tmp/helper.o
"$KEEL" archive /tmp/helper.o -o /tmp/libhelper.a
"$KEEL" link /tmp/start.o -L /tmp -lhelper -o /tmp/answer
/tmp/answer; echo $? # 42
```

## Validation

Run these inside the container:

```sh
cargo test -p keel-c --test environment --test run
cargo test -p keel-codegen --test run programs -- --exact
cargo test -p keel --test aarch64_linux_compiler --test aarch64_elf
cargo test -p keel-asm --lib --test structured --test aarch64_elf
cargo test -p keel-link --test aarch64_elf
```

Native execution cases run only on Linux AArch64. Coverage includes:

- All C fixtures (ordinary and optimized) and the portable IR fixtures across
  allocator, optimization and selection configurations.
- Clang interoperability in both directions.
- An ABI matrix over both allocators, optimization on/off and three object
  routes (direct objects, Clang assembly, Keel assembly): structs, HFAs, i128,
  register exhaustion, over-alignment, narrow stack arguments, variadics,
  `va_copy`, passed `va_list`, libc formatting and TLS across pthreads. LLVM
  verifies each callee's DWARF.
- A forced-spill TLS test, a freestanding link/execute check for
  compiler-produced objects, and header, relocation, archive and diagnostic
  checks.

The Clang overflow reference fixture (`__muloti4`) needs the compiler-rt package
and selects `--rtlib=compiler-rt`. The full linker test suite also contains
x86-only execution cases, so use the focused ARM commands above.
