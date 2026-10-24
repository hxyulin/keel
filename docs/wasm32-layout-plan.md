# wasm32 C layout design

This is the design record for adding the wasm32 ILP32 data layout to Keel. It
explains the decisions behind the layout, pointer and target model; the resulting
behavior is documented in:

- [IR storage layout](wasm-ir-layout.md) (`DataLayout`, layout-aware verification)
- [C data model](wasm-c-layout-foundation.md),
  [integer evaluation](wasm-c-integer-evaluation.md),
  [declarations](wasm-c-scalar-declarations.md),
  [scalar lowering](wasm-c-scalar-lowering.md) and
  [pointers and data](wasm-c-pointer-data.md)
- [target identity and gates](wasm-target-foundation.md)
- [freestanding C profile](wasm-c-freestanding.md) and [CLI](wasm-cli.md)

See also the original [feasibility evaluation](wasm-feasibility.md).

## Approach

1. Add explicit IR memory-layout queries first, without a new `Target` variant.
2. Migrate width-dependent C decisions to an explicit per-compilation model.
3. Expose `Wasm32UnknownUnknown` only together with frontend rejection gates and
   explicit rejection in every native entry point.
4. Start C with no optimizer passes.

Keep `ptradd(ptr, i64)`, `ptrtoint -> i64` and `inttoptr(i64)` in the IR and define
wasm32 normalization at those operations, rather than changing verifier operand
types or making `Type::Ptr.bytes()` host-dependent.

Key constraints:

- `DataLayout::WASM32` (4-byte pointers) and `DataLayout::NATIVE64` (8-byte) are
  storage descriptions only, not ABI classifiers or runtime configuration, and are
  never inferred from the host.
- A module has no target field. The caller owns the target/layout pairing; raw IR
  cannot prove whether its field offsets were computed for LP64 or ILP32. Native C
  IR must never be relabeled as wasm32, and target-tagged artifact transport stays
  unavailable for Wasm.
- The Wasm emitter takes `&Module` plus explicit options and returns complete bytes
  or a diagnostic.

## IR layout API

The IR gains a `DataLayout` type with exactly two constants and pointer size and
alignment queries, plus `*_with_layout` variants of the `Layout`, `DataItem` and
`DataInit` size, alignment and leaf queries (see [IR layout](wasm-ir-layout.md)).
Existing no-argument methods delegate to `NATIVE64`, and `PTR_BYTES = 8` remains as
a compatibility constant. Recursion passes the same layout through every field
alignment, array stride, padded size and leaf; addition, rounding and
multiplication are checked.

Notes on specific items:

- `DataItem::Ptr` is four bytes under `WASM32`. Integer items keep their typed
  width; byte and zero items keep their counts.
- `RttiName` is a native tagged eight-byte item, not a portable pointer. It stays
  eight bytes and Wasm backends reject it.
- Native consumers (`keel-codegen`, `keel-asm`, ABI code) keep using the native
  wrappers; uses of `PTR_BYTES` were not mechanically replaced.
- There is no `Layout::Union`. C unions and packing already have explicit offsets in
  `StructDef`; `Types::abi_layout` is a native ABI approximation, not a source of
  Wasm field offsets.

Reference values: `{ptr, i8}` is 8/4 with leaves at 0 and 4 on wasm32, and 16/8
with leaves at 0 and 8 natively. A pointer plus i32 initializer is 8 bytes on
wasm32 and 12 natively.

## C model

The public `keel_c::compile(src, path, Options)` and `Options { debug_info, target }`
are unchanged. Internally, `CDataLayout` (`NATIVE_LP64` or `WASM32`) controls the
width of `long` and pointer storage, not nominal type identity or native char and
long-double policy. The model is stored in the parser and used by the generator,
and must be threaded through every width-dependent helper: literal typing, usual
arithmetic conversions, constant normalization and evaluation, folding and truth
proofs, overflow builtins, initializer packing and bit-field containers.

Decisions:

- wasm32 `long` is i32, `long long` stays i64, plain `char` is signed.
- Keep rank `int < long < long long` even though `int` and `long` are both four
  bytes. `unsigned int + long` is `unsigned long`; `unsigned long + long long` is
  `long long`. Unsuffixed and `L`/`UL` literal candidate lists gain `long long`
  alternatives; LP64 choices are unchanged.
- `sizeof` yields `unsigned long` and pointer difference `long`, both 32-bit.
  Subtract zero-extended addresses in i64 and divide by the element size
  **before** truncating; a defined pointer difference fits in `long`.
- GNU `DI` selects `long long`; `word`/`pointer` select `int`; `SI`, `QI`, `HI`,
  `TI` keep their nominal types; unsigned bases select unsigned counterparts.
  Native LP64 keeps `long` for `DI`/`word`/`pointer`.
- Ordinary struct, union and array layout uses the model, keeping explicit
  alignments. Bit-fields and packed records are diagnosed on wasm32 until their
  target rules have oracles; the AAPCS64 bit-field algorithm is not assumed to
  apply.
- Scalar ABI types, SSA locals, pointer spill slots, casts and loads/stores use the
  model. Debug info encodes native widths, so Wasm debug output is rejected.

A `sizeof`-only test is insufficient; the fixture below checks nominal identities.

## Pointer initializers

Symbolic pointer relocations use the compilation's pointer width for size,
alignment and cursor advance together. In the first subset, symbolic relocations
are allowed only in pointer-typed storage; integer-valued symbolic address
initializers are diagnosed. Numeric null and high-bit pointers are normalized and
written as exactly four little-endian bytes. The u32 `DataItem::Ptr.addend` and
rejection of negative or oversized addends are kept, and relocation bounds and
overlap are validated before slicing.

For `struct { char tag; char *p; long n; } = {1, data+3, 7}` the wasm32 items are
four bytes of tag and padding, a four-byte symbolic pointer at offset 4 and the
four-byte `long` at offset 8: total 12, alignment 4. Arrays of these with adjacent
sentinels catch a stale eight-byte cursor advance.

Function-address relocations, indirect calls, external data, `RttiName`, TLS and
native section/linkage metadata are rejected, including on unused objects, until
table and import support exist.

## Pointer semantics

For the wasm32 profile, every pointer obeys:

- Pointer values, locals, loads and stores are i32; null is zero.
- `inttoptr(i64 x)` takes the low 32 bits. `ptrtoint(p)` zero-extends the unsigned
  i32 representation to i64, including addresses with bit 31 set.
- `ptradd(p, i64 d)` computes `(u32(p) + low32(d)) mod 2^32`. Negative offsets work;
  a C dereference still requires a valid object.
- Pointer comparisons compare normalized unsigned address bits, consistent with
  `inttoptr` truncation. Function pointers are out of scope.
- Pointer addition is done in i32 arithmetic before the load or store. Wasm's
  memarg offset addition does not wrap, so folding a wrapping `ptradd` into a memarg
  offset can change traps. Use offset zero, and test `0xffffffff + 1`, negative
  offsets and out-of-bounds widths.

These are Keel target semantics, not claims that all C pointer arithmetic is
defined. See the
[Wasm memory execution rules](https://webassembly.github.io/spec/core/exec/instructions.html#memory-instructions).
`ptrtoint(inttoptr(x))` is **not** an identity on wasm32. Frontend constant
evaluation must agree with backend normalization. The IR text syntax is unchanged,
and raw IR compilation must select the target explicitly.

## ABI, macros and headers

The target follows the
[Basic C ABI](https://github.com/WebAssembly/tool-conventions/blob/main/BasicCABI.md)
for freestanding wasm32: ILP32, pointer alignment 4, `long long` 8/8,
`long double` binary128 16/16, and a 16-byte-aligned software stack. Aggregate,
complex and vector call boundaries and variadics are rejected. The Emscripten
long-double alignment variant is not claimed.

The [Clang WebAssembly target](https://github.com/llvm/llvm-project/blob/main/clang/lib/Basic/Targets/WebAssembly.h)
confirms `unsigned long` `size_t`, `long` `ptrdiff_t`/`intptr_t`, a `void *`
`va_list`, binary128 `long double`, and `long long` for explicit 64-bit types.
Large array **objects** need at least 16-byte placement alignment for local and
global arrays, while array type and member alignment stay natural. (Clang's
`__alignof__` of a `char[17]` reports 1; that expression does not describe
allocation placement.) VLAs and dynamic stack allocation are rejected.

Macros: a dedicated wasm32 branch emits `__wasm__`, `__wasm32__` and Clang's
unadorned aliases, `__STDC_HOSTED__=0`, correct long/pointer/size widths and limits,
integer suffix and type families (including signed `long` `sig_atomic_t`), an empty
user label prefix, and binary128 `LDBL` macros (mantissa 113). Explicit 64-bit and
`intmax` families use `long long`. No LP64, Apple, ARM/NEON, x86/SSE/AVX,
Linux/ELF/Unix, WASI or Emscripten macros are emitted. Clang's default optional
Wasm features and lock-free atomic macros are not copied: atomics and threads are
unsupported, so `__STDC_NO_ATOMICS__=1` is defined, capability queries are
conservative, and atomic declarations and operations are diagnosed. SIMD source
types and operations are rejected rather than scalarized.

Headers: user `-I` and explicit `-isystem`, then embedded portable headers. No
automatic SDK, framework, `/usr/include` or sysroot lookup, and `--sysroot` is
rejected. Portable `stddef`, `stdint`, `limits`, `float`, `stdbool`, `stdalign`,
`stdnoreturn` and declaration-only `stdarg` are available; compiler-owned x86 and
private intrinsic headers are diagnosed. Explicit user include paths are the user's
inputs.

`va_list` is a `void *` declaration so `stdarg.h` parses; `va_start`/`va_arg`/
`va_end`/`va_copy` and variadic callables are rejected, and the non-Apple SysV
fallback is never taken. `long double` uses the existing opaque 16/16 type (complex
`long double` 32/16). Typedefs, `sizeof`/`alignof`/`offsetof`, pointer formation
and unused prototypes may parse; evaluated values, conversions, extended-FP
literals, definitions or calls with extended-FP parameters or results, and
aggregates containing them are rejected before dead-code pruning. This format is
never lowered to F64, F80, i128 arithmetic or an implicit helper.

## Variant and CLI migration gates

Adding a fourth `Target` is unsafe even when exhaustive matches compile:

- Exhaustive matches (`Target::ALL`/`triple`, section validation, native codegen
  dispatch, the CLI's vector target, object sinks) force decisions. Each native
  consumer (text, typed/chunked/object codegen, section validation, assembler,
  linker, archive) needs an explicit unsupported-target error. Wasm must never map
  to AArch64 or x86 selection.
- Equality tests and default branches do **not** force review: predefined macros
  and `va_list` fall through to native defaults, aggregate classification defaults
  to `abi_layout`, and x86 feature detection treats non-x86 as baseline.
  `Target::ALL` also expands test loops. Every usage needs an audit.
  `Target::host`/`Default` stay native.
- `Target::data_layout()` maps the three native targets to `NATIVE64` and Wasm to
  `WASM32`. `from_triple` accepts exactly `wasm32-unknown-unknown`, with no WASI or
  wasm64 alias.

The CLI routes Wasm at `cc`/`compile` dispatch, before native tuning validation,
native optimization context and selector/allocator setup. Both dispatch arms
optimize before emission, so guarding only the emit step would be too late
(including for `cc --emit-ir`). Wasm uses a distinct complete-module output mode
(`--emit-wasm -o file.wasm`) and rejects `--emit-object`, native assembly, CPU
tuning and features, register allocation and selection options, `-g`, `-O`,
explicit passes and incompatible report options. Native CLI flags and default
outputs are unchanged.

## Optimizer safety

`VectorTarget::Portable` is not a data layout, and `OptimizationContext` carries
only SIMD target and capability information. Wasm is therefore gated at the CLI
before any default or requested pass, including `opt --target wasm32...` and report
paths, and C output is unoptimized by default.

Known layout hazards: `stack_promote` and `stack_forward` treat unsized pointers as
eight bytes; `instcombine` folds and groups pointer offsets; `const_fold` not
folding `ptradd` is not proof that other passes are safe. Memory alias, forwarding
and load-narrowing passes, loop pointer ranges, `adjacent_diff`, `loop_vectorize`,
private aggregate/internal fast-call rewrites and automatic vectorization all need
proofs and tests before being enabled. Unsupported architectural operations must
fail admission even in unused code. The eventual opt-in profile is described in the
[basic optimization audit](wasm-basic-optimization-audit.md).

## Test expectations

Target gating: diagnostics for `-O`, explicit passes, native output paths and
native library APIs; no host SDK or sysroot discovery, native macros or intrinsic
headers; portable header typedef identities and limits; unused extended-FP
prototypes accepted but definitions and calls rejected; variadic, aggregate,
atomic, SIMD, inline-assembly and dynamic-stack rejection; native CLI and layout
controls.

Memory and runtime: null, high-bit and truncated pointers, negative offsets and
pointer differences, pointer arrays and initializers, recursion and SP restoration
on all normal returns, stack/data overlap and exhaustion, OOB traps, volatile
ordering and forbidden symbol kinds. Four-byte pointer storage does not authorize
reusing a native call ABI.

## Declaration oracle

[`suite/wasm32-layout/declarations.c`](../suite/wasm32-layout/declarations.c) is an
independent Clang declaration and layout oracle (no objects, linking or Wasm
engine):

```sh
clang --target=wasm32-unknown-unknown -std=gnu11 -ffreestanding -nostdinc \
  -Werror -fsyntax-only -Xclang -fdump-record-layouts-complete \
  suite/wasm32-layout/declarations.c
clang --target=wasm32-unknown-unknown -ffreestanding -nostdinc -dM -E \
  -x c /dev/null
```

Its assertions cover integer and mode identities and nested offsets, including Pair
12/4, Payload 16/8, Nested 56/8, Extended 48/16 and pointer initializer member
offsets 4 and 8. It records explicitly that `__alignof__(large_array)` reports
natural alignment, not placement alignment. The macro dump shows Clang's optional
Wasm feature and lock-free atomic macros, which Keel deliberately does not adopt.
