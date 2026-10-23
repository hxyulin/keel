# Remaining userspace ISA work

The primary release goal is ordinary userspace x86-64-v1, then v2, then v3.
Baseline NEON IR/codegen is already accepted in its supported scope. See
[the current support status](userspace-isa-status.md).

The next completion review should join the existing accepted userspace evidence
to the cumulative psABI feature lists, ordinary ABI behavior, feature legality
and documented value/memory/exception contracts. It should identify concrete
userspace gaps before requesting more implementation.

Kernel instructions, CPL0 guests, arbitrary physical registers/frames, full
saved FLAGS images and signal-resumption experiments are optional tracks. Do
not keep userspace completion or unrelated development waiting on them. Their
unfinished or failed proofs remain unfinished or failed in the archive.
