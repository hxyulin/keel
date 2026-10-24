# Remaining userspace ISA work

The primary goal is ordinary userspace x86-64 at psABI levels v1, v2 and v3.
Baseline NEON IR/codegen is complete in its supported scope. See
[userspace ISA status](userspace-isa-status.md) for current coverage and
known limits.

Remaining userspace work is driven by comparing the cumulative psABI feature
lists, ordinary ABI behavior, feature legality and documented
value/memory/exception contracts against the existing tests, and closing
concrete gaps found that way.

Kernel instructions, CPL0 guests, arbitrary physical registers and frames, full
saved FLAGS images and signal-resumption experiments are optional tracks. They
do not block userspace support, and their unfinished parts are documented as
unfinished in their own pages.
