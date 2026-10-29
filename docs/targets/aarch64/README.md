# AArch64

This page covers the AArch64 Linux toolchain, NEON lowering and intrinsics, and
branch lowering.
For shared compiler behavior see [architecture](../../compiler/design.md) and
[SIMD IR](../../compiler/simd-ir.md). The [assembler](assembler.md) and
[C inline assembly](c-inline-asm.md) have their own pages.

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

## Stack argument addressing

Darwin and Linux AArch64 support argument areas larger than 4,080 bytes for
incoming parameters and direct or indirect calls. Loads and stores use scaled
immediate offsets when they fit and form a scratch address otherwise. Aggregate
copies follow the same rule for their final narrow pieces. Outgoing argument
areas still count toward the allocator's 32,760-byte spill-area bound and the
function's frame-size bound; guaranteed tail calls still require register-only
arguments.

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
  cargo test -p keel --test aarch64_linux_compiler aarch64_elf::
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

## NEON lowering and calling convention

D and Q values use exact-width 8- and 16-byte memory, copies and spill homes.
Packed operations cover arithmetic, comparisons, lane selection, widening and
narrowing, reductions and common permutations; shared legalization supplies
other portable recipes. Exact static ZIP, UZP, TRN and window permutations
select native forms.

Named vectors share v0–v7 with scalar floating arguments; overflow arguments
keep each value's size and alignment, and results use v0. Apple variadic D/Q
values use exact 8/16-byte stack slots, and vector `va_arg` aligns and advances
them accordingly. Homogeneous vector aggregates of up to four members use one
SIMD register per member when the bank has room, and the stack otherwise.
Other aggregates use the indirect-copy and x8-result ABI. Upper SIMD halves are
not assumed preserved across calls.

## NEON intrinsics

`neon_<operation>.RESULT(args...)` is a typed ARM intrinsic. Every operation has
an explicit accepted result type, operand types and arity in
`NeonIntrinsic::operand_types`; x86 rejects these target-specific operations.
Examples include `neon_sqadd.i8x16(a,b)`,
`neon_smlal.i64x2(accumulator,low_i32_vector_a,low_i32_vector_b)`, and
`neon_fcvtzs.i32x4(float_vector)`. Long operands use their low half; narrow
results zero all unused upper lanes. Architecture-defined rounding, doubling,
intermediate saturation, NaN handling and accumulation are those of the named
baseline ARM instruction. FMLA/FMLS take accumulator first, then multiplicands
in written instruction order. They round once; generic multiply/add remains
unfused. Immediate shift and fixed-point forms take a final literal `i32` SSA
constant whose range is verified. Dynamic counts use the separate variable
shift intrinsics, which interpret the signed low byte of each count lane;
portable shifts continue to mask counts modulo lane width.

Explicit intrinsics execute with the current FPCR and update FPSR, including
sticky QC and exception flags, according to the named ARM instruction. They are
pinned barriers, conservatively may trap, and retain status effects when their
value result is unused. `neon_read_fpsr.i64()` and `neon_read_fpcr.i64()` read
state. `neon_swap_fpsr.i64(value)` and `neon_swap_fpcr.i64(value)` return the
previous register value and then write the supplied value, following ARM rules
for reserved register bits.

**Portable floating operations retain their pre-existing numerical contract;
their interaction with FPSR is unspecified.** In particular, mixing `vfadd`,
scalar floating arithmetic, or other portable FP operations with an explicit
FPSR read/swap does not make their intermediate exception flags observable IR
semantics. Their scalar or packed implementation and optimization can change
those incidental flags. Clients requiring architectural FPCR/FPSR behavior must
use explicit architectural intrinsics for the operations they observe. Portable
integer saturation remains independently specified to preserve QC.

Baseline half-storage conversions use `i16x8` to carry IEEE binary16 bit patterns;
this does not add optional FP16 arithmetic types or instructions. Numeric/crypto
extensions, SVE and SME remain outside the baseline inventory.

### D transport and structured memory

The six D shapes have 8-byte size and alignment. ARM ordinary memory,
spills, stack arguments, and homogeneous vector aggregate members transfer
exactly eight bytes. Named arguments and results use the low SIMD register
bits. Packed architectural D forms use explicit Q=0 encodings; single-lane
64-bit architectural forms use the corresponding scalar encoding.

Structured memory operations use the existing `neon_` instruction syntax,
with a vector shape suffix. `neon_ld2`, `neon_ld3`, and `neon_ld4` return
2–4 vectors in structure order; `neon_ld1` returns one. `neon_ld1x2`,
`neon_ld1x3`, and `neon_ld1x4` read contiguous vector lists. Stores take the
address followed by the vectors and have no results. `neon_ldNr` reads one
lane per structure and replicates it. `neon_ldN_lane` takes an address,
N original vectors, and a literal i32 lane index, returning N updated vectors;
`neon_stN_lane` takes the same inputs and returns nothing. N ranges from 1
through 4. All six D and six Q shapes are supported; lane indices are checked
against the shape. Address writeback is an explicit `ptradd` in SSA.

For example:

```text
v2, v3 = neon_ld2.i16x4(v0)
neon_st2.i16x4(v1, v2, v3)
```

These instructions remain pinned memory barriers. They make no alignment or
fault-freedom promise and are not atomic. Full transfers access exactly N
vector widths; lane and replicate transfers access exactly N element widths.
The backend selects native list, interleaved, lane and replicate structure
instructions where their arrangements are legal, retaining exact scalar
legalization for arrangements without a native structure encoding. Post-index recipes
perform the pinned memory access before computing the next SSA pointer with
`ptradd`.

### Scalar architectural forms

Explicit NEON saturation accepts scalar i8/i16/i32/i64 where the scalar
instruction defines that size. Plain variable/immediate shifts accept i64;
saturating scalar shifts accept all four widths. Doubling multiply-high
accepts i16/i32, doubling long multiply/accumulate i32/i64 results, and
saturating narrowing i8/i16/i32 results with twice-width inputs. Raw scalar
FP operations and conversions use f32/f64 and i32/i64 as appropriate. Scalar
pairwise FP results consume f32x2 or f64x2. One-lane D64 forms use these scalar
encodings rather than reserved Q=0,size=64 vector encodings.

Integer SSA values are transferred into and out of low SIMD lanes without
arithmetic conversion. All explicit operations retain pinned FPCR/FPSR
semantics, including when the numeric result is dead. Scalar `neon_fmla` and
`neon_fmls` use architectural fused instructions with accumulator first and
written multiplicand order. Indexed recipes extract a lane then invoke these
pinned operations. Portable FMA does not supply this architectural-status
contract. `neon_frecpx` accepts f32/f64 and one-lane f64 vectors.

`neon_cls` and `neon_rbit` expose the baseline sign-count and per-byte bit
reversal operations. `neon_mla`, `neon_mls`, `neon_saba`, and `neon_uaba` take
the accumulator first; `neon_sadalp` and `neon_uadalp` consume a same-size
vector of half-width source lanes after the accumulator. Accepted D/Q lane
widths are checked per operation. These architectural intrinsics preserve
FPSR for the integer operations whose instruction definitions leave it alone.

Optional FP16 arithmetic, crypto, SVE and SME are separate from baseline NEON.

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
cargo test -p keel --test aarch64_linux_compiler
cargo test -p keel-asm --lib --test structured
cargo test -p keel-asm --test assembler aarch64_elf::
cargo test -p keel-link --test elf aarch64_elf::
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

## Ordinary zero branches

For an ordinary i32 or i64 `brif` truth test that compare fusion does not
cover, the backend emits CBZ to the false destination instead of
`cmp reg, #0` and B.cond. On i32, CBZ on the W register tests the same 32 bits;
on i64 it tests all 64. Zero is false and every nonzero pattern is true. Far
branch relaxation may invert CBZ to CBNZ over B. i8 and i16 keep a masked TST
because their upper bits are undefined, and i128 keeps ORR of both halves and
CMP. A function containing inline assembly keeps CMP and B.cond for every
`brif`, because a template can read NZCV without declaring it. The proof that
no other generated sequence reads the removed flags is documented on
`BrifTest` in `crates/keel-codegen/src/aarch64.rs`.

```sh
cargo test -p keel-codegen --test arm_zero_branches
```
