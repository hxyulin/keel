# Bounded raw x87 comparison contract

`x86_x87_fcom`, `fcomp`, `fcompp`, `fucom`, `fucomp`, and `fucompp`
(with the `x86_x87_` prefix on each name) take two F80 values and return
one i32 containing the zero-extended raw post-principal status word.
`x86_x87_fcomi`, `fcomip`, `fucomi`, and `fucomip` take the same inputs
and return `(i32 flags, i32 status)`. The flags result is an architectural
bit mask: CF=bit 0, PF=bit 2, ZF=bit 6; all other result bits are zero.
The six non-I forms expose relation in SW.C0/C2/C3. Ordinary comparisons
and their language predicates are unchanged.

Inputs occupy logical ST(0), ST(1), in argument order, loaded from existing
F80 stack homes in reverse order. Each named principal instruction executes
once even if every output is dead. Its pop count affects the returned SW.TOP;
cleanup then balances the two temporary stack entries. FNSTSW immediately
follows the principal, before cleanup. I forms capture RFLAGS using PUSHFQ
and reserved RCX; no LAHF feature dependency or allocated GP clobber is added.
Both inputs are consumed before scalar results are published, including
source/result shared homes. Status is published before flags so spilled
results cannot overwrite the saved AX status.

The bounded entry contract is empty x87 tags, arbitrary TOP, inherited CW
with all exceptions masked, and any inherited PC/RC and sticky exception
bits. CW is preserved, final TOP equals entry TOP, and tags remain empty.
Unmasked exception delivery and nonempty incoming x87 stacks are outside
this contract. FLD m80 loads preserve raw extended payloads (including
unsupported encodings); their architectural effects precede the compare.
FCOM-family forms signal invalid for QNaN as well as SNaN/unsupported
encodings. FUCOM-family forms suppress invalid for QNaN but signal it for
SNaN/unsupported encodings. Masked invalid produces unordered relation.

The raw SW return includes diagnostics that are not promises about undefined
condition bits. Native acceptance compares defined SW fields separately:
non-I forms define relation/C1 in addition to exceptions and TOP; I forms
exclude C0/C2/C3 from acceptance. Full raw words remain diagnostics.
Integer-memory FICOM/FICOMP recipes and full profile completion remain open.
