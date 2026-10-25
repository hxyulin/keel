# Remaining userspace ISA work

The primary goal is ordinary userspace x86-64 at psABI levels v1, v2 and v3.
Baseline NEON IR/codegen is complete in its supported scope. See
[userspace ISA status](userspace-isa-status.md) for current coverage and
known limits.

Remaining userspace work is driven by comparing the cumulative psABI feature
lists, ordinary ABI behavior, feature legality and documented
value/memory/exception contracts against the existing tests, and closing
concrete gaps found that way.

Baseline privileged/system instructions are now an active separate goal. See
[system instruction work](x86-system-support.md) for implemented operations,
contracts and remaining work. CPL0 guests, physical registers and frames, saved
FLAGS images and signal resumption need their own validation. Their unfinished
parts do not change the userspace support contract.
