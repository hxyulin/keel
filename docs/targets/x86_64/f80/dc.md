# Raw DC destination-ST(1) arithmetic

Six distinct APIs take `(a:F80,b:F80)` and return `(destination:F80,i32 rawSW)`.
Physical operands are always pre-op ST(0)=a and ST(1)=b, loaded as FLD b, FLD a.
The principal instruction is non-popping with destination ST(1), source ST(0).

| Intrinsic | Intel principal instruction | Exact bytes | Result |
|---|---|---|---|
| x86_x87_fadd_st1 | fadd st(1),st(0) | DC C1 | b+a |
| x86_x87_fmul_st1 | fmul st(1),st(0) | DC C9 | b*a |
| x86_x87_fsub_st1 | fsub st(1),st(0) | DC E9 | b-a |
| x86_x87_fsubr_st1 | fsubr st(1),st(0) | DC E1 | a-b |
| x86_x87_fdiv_st1 | fdiv st(1),st(0) | DC F9 | b/a |
| x86_x87_fdivr_st1 | fdivr st(1),st(0) | DC F1 | a/b |

No D8/DE alias or operand-load exchange is permitted. Destination identity,
NaN operand priority and immediate status follow the actual DC opcode even
where the mathematical expression matches another form. FNSTSW AX immediately
follows the principal opcode, returning the unmasked raw 16 bits zero-extended.
Snapshot TOP is `(entry_TOP-2)&7`: both physical operands remain occupied.
Cleanup first pops the original ST(0), then stores/drops the ST(1) result.
This restores entry TOP and empty tags. Both inputs are loaded before any
value/status output home can overwrite them; accesses use exactly ten bytes.

All six APIs are pinned barriers even with unused outputs. The caller supplies
empty tags with arbitrary TOP, a valid CW with all six exception masks set and
no pending unmasked exception. CW including PC/RC is inherited and preserved.
FLD effects are included in the snapshot; later cleanup effects are excluded.
Hardware-defined values and status follow the exact opcode. C0/C2/C3 and
exceptional C1 remain hardware-undefined, never canonicalized; numerical-domain
and rounding gates for native proof must be explicit. No libm substitution or
changes to generic F80 arithmetic are made.

Primary source: [Intel SDM vol.2A](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf),
FADD (3-312), FMUL (3-361), FSUB (3-395), FSUBR (3-398),
FDIV (3-334) and FDIVR (3-337) register-form encoding tables.

The tests compare against an independent assembler DC byte fixture and compile
every liveness/selector/allocator combination; native numerical/status
validation is still outstanding:

```sh
cargo test -p keel-codegen --test x86_x87_dc
```

Memory-operand arithmetic is not supported. Supporting it would require exact
F32/F64 operand widths, principal memory-opcode load/conversion exceptions,
source order, NaN selection, immediate status and guarded accesses; FLD plus
register arithmetic is not a substitute. Arbitrary stack indices are also not
supported; the eighteen fixed-register arithmetic APIs (here and in
[binary.md](binary.md)) are not full x87 coverage.
