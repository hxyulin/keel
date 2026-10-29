# x86 LEA selection for multiply-by-three plus constant

Rule: `iadd(imul(x, 3), C)`, with either operand order at both nodes, modular
i32/i64 only. The product must have exactly one resolved operand occurrence,
belong to the root block, and have no distinct explicit source line. The root
result must not be an alias. i32 factor/displacement constants are reduced
modulo 2^32; an i64 displacement must fit signed disp32. Only factor three is
selected; factors five and nine remain unchanged.

Selection is enabled only by the existing trees options (`SelectionTier`
Trees/Full, `--isel trees`/`all`, or the `-O` selection preset). Baseline,
folds-only and hints-only selection do not activate it. Default pipeline
settings are unchanged.

`Machine::LeaMul3` carries the signed displacement plus one resolved input in
its match arguments. The product is covered; constants are omitted from root
allocation operands, and existing folding decides whether their definitions
remain needed. The x86 emitter loads the input once and emits
`LEA r32/r64, [r64 + r64*2 + disp32]`. The 32-bit destination truncates the
effective address and zeroes its upper half, matching existing i32 result
handling. LEA performs no memory access and neither reads nor writes flags; no
IR operation exposes IMUL/ADD flags.

`bridge_madd.ir` is the `bridge_madd` function extracted from the C frontend's
optimized x86-64 Linux output for
`crates/keel-opt/tests/load_narrow/recurrences.c`. As a retained fixture it does
not by itself establish activation by a fresh frontend build.

## Tests

```sh
cargo test -p keel-codegen --test x86_scalar_selection -- x86_lea::
```

The tests check exact factors, widths, displacements and operand orders;
fallback for shared, cross-block and located products; alias resolution; and
that the retained `bridge_madd` IR selects LEA only in tree modes and assembles
for both targets. On x86-64 Linux hosts, an additional test links the direct
i32/i64 functions (signed disp32 endpoints, wraparound, rejected factors, small
register pools with spills) against a separately compiled Clang `-O0 -fno-lto`
oracle and executes them.

## Remaining validation

Fresh-frontend activation on `recurrences.c` (the `bridge_madd` loop using
`LEA state + state*2 + 7` under trees/full while baseline keeps IMUL/ADD) and
native execution of the full `bridge_madd` loop against an independent oracle
are not covered by these tests.

No performance or application gain is claimed. A handwritten x86 madd
microbenchmark suggested a lower time ratio (about 0.5), but that
single-session result does not transfer to generated loops.
