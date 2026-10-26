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
  --compiler gcc
```

Use a fresh export directory. Each reference compiler independently compiles the
GNU C asm fixture with renamed functions. The runner links each Keel object with
that reference and the C comparison harness. Every executable must return zero
and print exactly `PASS`. It checks 512 inputs per function, including integer
edges, narrow values, live values and memory canaries. Fifteen deliberately wrong
implementations per reference compiler must return one with a `FAIL` transcript;
crashes and timeouts never count as a control passing.

The current fixture uses sixteen simultaneous flag outputs, which the local
Clang C frontend rejects for register pressure. GCC is the qualified reference
for this fixture. Additional `--compiler` options require a compiler that
accepts the complete source; earlier Clang results below cover earlier fixtures.

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

The memory-operand extension adds eleven functions, for 35 exported functions in
the current fixture. They cover `m`, `=m` and `+m`, mixed operand numbering,
register ties, aliasing addresses, byte/word/dword memory operations, array
objects, single-evaluation addresses and aligned stack homes. The three new
broken controls omit a write, choose a wrong memory address or return a wrong
memory read. The historical native counts above precede this extension.

At memory-operand checkpoint `7572b6ef`, all 145 fresh objects passed against
GCC 13.3.0 on Linux x86-64, with all eight broken controls returning normal
failure. This qualifies the 35-function fixture above, including optimized and
unoptimized builds, both debug settings, both pools and the forced-rematerialized
configuration. A native Clang comparison of this expanded fixture was not run.

The q-class extension adds four functions, for 39 exported functions. They
exercise `=q`, `+q`, `=&q`, `+&q`, byte/word/dword/qword views and pointers.
With Keel's stack/frame registers reserved, clobbering the other six legacy
GPRs forces its byte operands into extended registers. Two more negative controls corrupt a q output and its byte result.
The previous native counts precede this fixture extension.

At q-class checkpoint `b60de289`, all 145 fresh objects passed against GCC
13.3.0 on Linux x86-64 across the 39-function fixture. All ten broken controls
returned normal failure. These are userspace witnesses; native Clang execution
of the expanded fixture was not run.

The address/negation extension adds six functions, for 45 exported functions.
They cover `%a` on pointer and 32-bit registers, numeric absolute addresses,
RIP-relative symbol addresses, and `%n` on positive, negative and minimum
signed 64-bit constants. Two controls corrupt the symbol addend and negated
offset. Native results for this extension are recorded separately below.

At formatting checkpoint `2959b536`, all 145 fresh objects passed against
GCC 13.3.0 on Linux x86-64 across the 45-function fixture. All twelve controls
returned normal failure. Native Clang execution of this extension was not run.

The choice/flag extension adds ten functions, for 55 exported functions. It
covers `iq` immediate/register fallback after mandatory inlining, scalar `rm`
and `g`, aggregate `rm` inputs, symbolic choices and single-evaluation output
addresses. All sixteen conditions are captured together and checked against
an independent C calculation of comparison flags. Mixed cases combine narrow
flag results, register ties and memory outputs. Three new controls corrupt a
choice immediate, a flag condition and a memory write. GCC accepts this GNU
fixture; the local Clang C reference rejects its sixteen simultaneous flag
outputs. Clang/GNU assembly comparisons remain separate from native C execution.

At choice/flag checkpoint `d14e3958`, all 145 fresh objects passed against
GCC 13.3.0 on Linux x86-64 across the 55-function fixture. All fifteen controls
returned normal failure. Every object also passed the independent C flag-bit
oracle. Native Clang execution was not run; its local C frontend rejects the
sixteen concurrent flag outputs used by this fixture.
