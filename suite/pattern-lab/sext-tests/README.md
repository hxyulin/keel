# Opt-in sign-extension correctness capture

Run only under an explicit root MacHost compiler/native correctness lease, with
a compiler built from the reviewed source in a private target. This script
never builds or measures performance. Supply an existing compiler and resolved
Clang executable, and a new output directory with an existing parent:

```
python3 suite/pattern-lab/sext-tests/validate.py --compiler /absolute/keel \
  --clang /absolute/clang --output /absolute/new-capture
```

All commands run serially with bounded subprocess deadlines, retained argv,
return code, stdout/stderr and command endpoint times. Any failed command or
assertion stops the capture; preserve that directory and use a new directory
for a corrected run. Review and seal tool/compiler/source/output identities
separately after releasing the host. The script does not install, contact
remote hosts, infer a source revision from a binary, or self-acquire a lease.

The source includes the actual `bridge_sext8` C body verbatim. For both targets
and full, baseline-selection, and source-debug modes, it captures optimized C
IR, explicitly requests `sign-extend-idiom`, and verifies the IR before codegen.
It asserts exact trunc/sext/jump dataflow in the non-debug positives, absence
of default activation, one eliminated byte branch, and unchanged fallback
functions (width, bounds, mask, subtraction, shared definition, volatile load,
noinline call, and multiple source paths). Source-debug locals retain memory
and locations, so those routes must fall back without relaxing debug guards.
The separate optimizer tests cover direct in-memory aliases and extra IR edges.

Only after every IR assertion passes does it generate both-target text and
direct objects, independently assemble the text with Clang, and link/run six
ARM routes. A separate same-source oracle translation unit is compiled with
Clang `-O0 -fno-lto`. The driver checks all bytes with four high-bit input
patterns, negative return values and call effects, and the bridge at counts
0, 1, 2, 17 and 257. Null data and maximal ignored mask are passed even for
nonzero counts because this family never accesses data. Positive return values
also use an independent mathematical recurrence. Each route performs 24,576
comparisons. Native x86 is not run on MacHost; x86 object validation is separate
from execution. Success is correctness evidence, not acceptance or speedup.
