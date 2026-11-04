# SysV tail programs

`x86_abi::x86_sysv_tail::compatible_tail_packet` builds all 24 selector,
allocator, register-pool and CPU-level combinations. It covers V1 and V3; V2
emits the same text as V1. Each combination is a `native_program!` built with
both GCC and Clang: compile `oracle.c`, link it with `depth.S` and the Keel
object, and require the exact output `KEEL SYSV TAIL PASS`. The V3 programs
require an x86-64-v3 host. On a host without x86-64 Linux, set
`KEEL_TEST_EXPORT_DIR` and run the packets with
`suite/integration/x86-exports/run.py`.

A constant-return replacement for one argument-permuting tail, linked with
`-Wl,--wrap=tail_gp_0_0`, must make the value checks exit with status 1, once
per compiler.

The C type roster is independent of Keel's classifier. It checks argument
reversal and duplication through direct and indirect tails, aggregate return
protocols, scalar tails, both exhausted register banks, i128 and x87 arguments,
constant stack depth, and sret aliasing. Checks exclude aggregate padding.
The interior-source overlap check only compares words within the first source
object; it makes no claim about adjacent byval storage.
