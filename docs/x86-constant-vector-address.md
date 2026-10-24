# Exact 16-byte x86 constant-address loads

Constant-offset address selection on x86 normally accepts accesses of up to
eight bytes; its 16-byte exception otherwise applies only to ARM vectors. This
document describes a bounded x86 exception that folds a constant `ptradd` into
an exact 16-byte vector load:

```text
v2 = iconst.i64 16
v3 = ptradd v0, v2
v4 = load.i32x4 v3    ; -> movdqu xmm0, [rdi + 16]
```

## Why the size gate is not simply `size <= 16`

x86 `i128` lowering emits two scalar m64 accesses and reads the original address
through `use_reg`, bypassing the selected-memory helper. Widening the generic
gate would therefore be wrong for `i128`. Other types, stores and indexed-address
selection each need their own audit.

## Contract

The exception in `select.rs` admits only:

- ordinary, nonvolatile `load.i32x4`;
- whose address is a sole-use, same-block `ptradd`;
- with an i64 constant offset that fits signed disp32 (zero and negative
  offsets included).

It keeps source-line boundaries, rejects a located address feeding an unlocated
load, and respects existing constraints. Offsets outside signed disp32 keep the
materialized 64-bit modular arithmetic. Displacements are never combined or
truncated. Signed disp32 is sign-extended in the 64-bit effective address,
matching `ptradd`'s modular arithmetic even for wrapping bases.

Two consumers use the selected address, both via `mem` with an exact m128 type:

- the ordinary emitter's single `movdqu`;
- the AVX2 `load.i32x4; swiden_low.i64x4` cover, which copies the selected
  load's operands to its root before allocation and emits one m128 `vpmovsxdq`.

No emitter change is needed. Only pure address construction disappears; memory
order, flags, alignment and access widths stay the same. The base stays live at
the selected consumer, including across calls and spill reloads. Shared
addresses remain materialized. Symbolic bases remain ordinary materialized
leaves; no relocation rewrite is added.

Not affected: other 128-bit vector shapes, every 256-bit load, vector stores,
volatile vector loads, atomics, `i128` and indexed vector addresses (the
indexed selector still accepts only 1/2/4/8-byte accesses). There is no
frontend, optimizer, default-width, FP, C ABI or feature-policy change. Ordinary
loads remain legal at v1/v2; AVX without AVX2 keeps its existing widening
fallback.

## Testing

```sh
cargo test -p keel-codegen --test x86_constant_vector_address
cargo test -p keel-codegen --release --test x86_constant_vector_address
```

The suite covers signed-disp32 and disp8 boundaries and adjacent rejected
values, i64 extrema, zero/negative offsets, sole/shared addresses, resolved base
aliases, cross-block and debug boundaries, symbolic-base object emission, all
selection-option combinations, exclusion of every other 128-bit vector shape
and a representative 256-bit shape. Private `packed_load` unit tests check leaf
transfer to the widening root and rejection at constraints and debug boundaries.

The exported corpus has 65 functions (13 offsets by five modes: plain, closed
widening, shared address, live result across a clobbering call, and a call
between `ptradd` and load) in 48 configurations: v1, v2, AVX without AVX2 and
v3; three selectors; two allocators; normal and empty register pools. Text,
typed and chunked objects must match and every instruction must fit its
effective feature set.

Setting `KEEL_ADDRESS_EVIDENCE=<dir>` exports the `.o`/`.c` pairs. Run them on
x86-64 Linux with `crates/keel-codegen/tests/x86_widen/run_native.py --dir <dir>`
(add `--cc gcc` for GCC). Each run checks input placements at both
protected-page edges and unaligned addresses, signed patterns including i32
extrema, exact 16-byte reads, overlapping input/output, output canaries,
crossing-page faults before any output write, live values across calls that
clobber all SysV caller-saved registers, and unchanged MXCSR. All 48 bundles
pass with Clang 23 and GCC 16 on x86-64 Linux.

## Measured effect

On the signed-sum fixture used by the ignored
`x86_avx2_direct_add::measure_signed_sum_direct_add` test (wide preference,
Full selection/allocation), the function shrinks from 102 to 96 instructions and
350 to 332 text bytes: three `mov`/`add` pairs disappear and the `vpmovsxdq`
operands become `[base + 16]`, `[base + 32]`, `[base + 48]`. Optimized IR is
unchanged.

Native timing of that kernel (x86-64 Linux, pinned core, twelve rounds), median
ns/call:

| Length    | Before    | After     |
|-----------|-----------|-----------|
| 16        | 3.478     | 3.463     |
| 64        | 4.621     | 4.282     |
| 256       | 9.501     | 8.863     |
| 4096      | 118.086   | 116.316   |
| 262144    | 7657.053  | 7455.558  |
| 1048576   | 30953.797 | 30287.037 |

This is a small hot-kernel experiment, not an application speedup claim or a
width-policy comparison. Execution was on an AVX2-capable host; it does not
certify v1/v2-only or AVX-only hardware.
