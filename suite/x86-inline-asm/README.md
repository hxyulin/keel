# Scalar x86 inline-asm execution witnesses

Export actual compiler-generated objects on any supported development host:

```sh
KEEL_X86_INLINE_EXPORT=/tmp/keel-inline-objects \
  cargo test -p keel-c --test x86_inline_asm
```

Copy that directory, `run.py`, `oracle.c` and
`crates/keel-c/tests/fixtures/x86_inline_asm.c` to a Linux x86-64 host. Then run:

```sh
python3 run.py --objects /tmp/keel-inline-objects \
  --source x86_inline_asm.c --work-dir /tmp/keel-inline-run \
  --compiler gcc --compiler clang
```

Use a fresh export directory. Each reference compiler independently compiles the
GNU C asm fixture with renamed functions. The runner links each Keel object with
that reference and the C comparison harness. Every executable must return zero
and print exactly `PASS`. It checks 512 inputs per function, including integer
edges, narrow values, live values and memory canaries. Five deliberately wrong
implementations per reference compiler must return one with a `FAIL` transcript;
crashes and timeouts never count as a control passing.

There are 144 ordinary configurations and one forced-rematerialization object.
At the scalar milestone, GCC 16.2.0 and Clang 23.1.3 passed all 145 objects: 290 positive
executions and four normally rejected broken controls. `report.json` under the
work directory records each verdict. These are userspace witnesses, not kernel
or privileged-execution qualification.

The current fixture adds required-inlined numeric and symbolic immediate helpers,
including nested helpers, repeated calls and `%=` labels. Its shared anchor is
defined by the independent harness, so both implementations must return the same
actual symbol-derived addresses. The historical native counts above precede
these additions. The expanded fixture passed 145 fresh native executions
against GCC 13.3.0 on Linux x86-64, with all five broken controls returning normal
failure. A fresh native Clang comparison remains outstanding; local Clang/GNU
assembler comparisons passed for the symbolic byte/relocation witnesses.
