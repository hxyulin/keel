Bounded x86 LEA selection checkpoint (2026-10-17)

Rule: iadd(imul(x, 3), C), with either operand order at both nodes,
modular i32/i64 only. The product must have exactly one resolved operand
occurrence, belong to the root block, and have no distinct explicit source
line. The root result must not be an alias. i32 factor/displacement constants
are reduced modulo 2^32; i64 displacement must fit signed disp32. Only factor
three is selected; factors five/nine remain unchanged pending separate evidence.

Selection is enabled only by existing trees options (SelectionTier Trees/Full,
--isel trees/all, or the existing -O selection preset). Baseline, folds-only,
and hints-only do not activate it. No default pipeline settings change.

Machine::LeaMul3 carries signed displacement plus one resolved input in its
Match args. The product is covered; constants are omitted from root allocation
operands. Existing folding decides whether their definitions remain needed.
The x86 emitter loads the input once and emits LEA r32/r64,[r64+r64*2+disp32].
The 32-bit destination truncates the effective address and zeroes its upper
half, matching existing i32 result handling. LEA has no memory access and
neither reads nor writes flags. No IR operation exposes IMUL/ADD flags.

bridge_madd.ir is the exact bridge_madd function extracted from retained
capture-002-final-4101626/x86_64-unknown-linux-gnu-opt.ir, from approved
commit 41016268a6c246905ba6b8d6191d44c75630d50f. It is actual C frontend
output, but a retained fixture cannot establish fresh compiler activation.

SOURCE CHECKPOINT ONLY: tests have not been compiled or run. Root's explicit
LINUXHOST correctness grant is required before any compiler/build/native/remote
work. Mandatory acceptance sequence after grant:

1. Record clean revision/tree, private remote checkout/archive receipt, tool
   executable hashes, exact commands/environment and every failed output.
   Build serially with -j2/private target; no MacHost work or performance.
2. Run dedicated x86_lea and existing selection/scalar_address/tiers tests.
   Check i32/i64 positive and negative text and structured objects on BOTH
   targets, including baseline/trees/full and allocator pressure/spills.
3. Compile actual suite/pattern-lab/compiler-cases/recurrences.c via .i bytes,
   retaining raw/optimized IR plus both-target text/object. Before native
   compilation, assert bridge_madd loop dataflow is LEA state+state*2+7,
   without the consumed IMUL/ADD, in trees/full; baseline stays IMUL/ADD.
   Verify rejected factors/immediates, multiple uses, widths, cross-block and
   debug-line cases remain fallback in fresh frontend and direct IR cases.
4. Compile an independent unsigned Clang -O0 -fno-lto oracle separately.
   Compare native x86 bridge_madd for n=0/1/2/3/7/31/257 and seeds around
   0, 2^31, 2^32, 2^63, UINT64_MAX and overflow boundaries, including null
   ignored data/n=0. Compare direct i32/i64 positive and negative functions
   for signed disp32 endpoints and wraparound. Exercise tiny pools and
   Fast/Full allocators, aliases and a live compare across the expression.
5. Retain exact binary/object/tool hashes and per-case comparison counts,
   release LinuxHost only after all jobs exit, notify root. No timing without a
   separate explicit root grant. Independent review/acceptance is root-owned.

The measured handwritten x86 madd ratio (~0.505) is conditional single-session
DATA only. The previous generated bridge retained IMUL+ADD (49 bytes), with
no native x86 run or generated-loop timing. This checkpoint asserts neither
runtime correctness nor performance nor an application gain.
