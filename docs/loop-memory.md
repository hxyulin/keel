# Guarded loop memory motion

`loop-memory` is an opt-in function pass that hoists an invariant ordinary load
out of a small, unrotated two-block loop through a cloned copy of the loop's
initial integer guard. No default pipeline runs it; schedule it explicitly with
`-p`.

## Motivation

C `while` and `for` loops lower to a condition header followed by the body, and
cleanup does not rotate them. A typical reload-style loop looks like:

```text
block0: jump block1(iv0, acc0)          ; entry / preheader
block1: cmp iv, bound; brif -> block2 / block4
block2: v = load.i64 p ; store q ... ; jump block1(...)
block4: return ...
```

A zero bound reaches `block4` without executing the load, so neither entry nor
the ordinary preheader is a legal destination for a potentially faulting load,
even with valid `restrict` contracts. `loop-memory` instead evaluates the first
header test before the loop, keeps the original zero-trip exit with exactly its
original arguments, and routes only the entered case through a dedicated load
preheader. The loop itself remains unrotated, and the original header test still
runs on every iteration.

## Accepted shape

- The header has exactly two predecessors: one unconditional external jump and
  one body back edge.
- The header compares a unit-step i32/i64 induction variable against an
  invariant bound, using at most 16 nontrapping integer instructions. The only
  cloned operations are integer constants, nontrapping integer binary
  operations and integer comparisons.
- The body is a single straight-line block of at most 128 instructions with
  exactly one ordinary load, whose address already dominates entry, before any
  store or other pinned or trapping effect.
- Later ordinary stores must be proved disjoint by the shared function-local
  `AliasInfo` helper (see [memory-forward.md](memory-forward.md)). Calls,
  volatile, atomic, lifetime and barrier effects and other traps reject the loop.
  Non-trapping annotations do not relax memory or lifetime dependencies, and
  `restrict` proves neither dereferenceability nor nonnull.
- The exit has exactly one incoming edge (from the header), ends in a return,
  and contains at most 32 instructions.

Header parameters and cloned header results must be their own resolved
identities. The `Function` API permits aliases on definitions still in layout;
such states are rejected conservatively so that a dead definition's clone cannot
overwrite the mapping of an invariant or another parameter. Clone-map keys use
raw definition identities, operands still resolve through aliases, and body
instructions are moved rather than cloned.

## Exit repair

Existing exit parameters and arguments are retained. Header values used
directly by exit instructions get additional typed exit parameters, in
deterministic operand traversal order. The original header false edge supplies
the original values; the cloned initial false edge supplies initial arguments or
cloned results. Only exit instruction operands are rewritten; there is no global
use replacement. Header-defined uses elsewhere reject the loop. Exit instruction
order and effects are unchanged.

Moving the load preserves its result identity and source location; cloned
instructions keep their original locations and scopes. Functions with
stack-variable debug descriptions are skipped.

## Limits

Whole-function analysis is limited to 512 arena blocks and 8192 laid-out
instructions, with at most eight loop transformations. CFG, dominance, loop
depth and alias information are recomputed before each candidate. Multiple
loads, body block parameters, nested loops, conditional bodies, complex exits
and store sinking are out of scope.

## Tests

```sh
cargo test -p keel-opt --test loop_memory
```

The IR tests cover direct header-value returns, multiple liveouts, both branch
orientations, locations, dominance, cleanup, conservative rejections, and
direct `Function` API alias states (aliases to an invariant bound, header
parameters and earlier results, chained aliases, duplicate resolved parameter
keys).

The C fixtures `crates/keel-opt/tests/loop_memory/reload.c` and `driver.c`
cover a reload-style recurrence, aliasing non-restrict accesses, legal
read-only restrict aliasing, conditional loads and a store-before-fault
fallback. The driver includes zero-trip null and protected-page inputs, trip
counts 0/1/2/7/8/257/4096 and sixteen seeds. Native output after the explicit
pass, and after subsequent cleanup, matches Clang O2 with the same driver on
arm64; x86-64 code generation is checked statically. No performance claim is
made for the pass.
