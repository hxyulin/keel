# Raw architectural x87 numerical packet

Run `python3 run.py` on native Linux x86_64 with distinct genuine GCC and Clang.
No libm and no C long double expressions occur in the oracle. probe.S executes
independent native instructions with an identical FNINIT/FLDCW prestate.
Every run compares ten-byte payloads within defined numerical domains, the
hardware-defined bits of zero-extended opcode-time status, final defined
exception/structural status, preserved CW and empty boundary tags. Raw status
is never canonicalized by the API. Undefined bits and numerical outputs outside
classified domains are observed separately as non-gating diagnostics, even if
independent executions on one CPU happen to agree.

F2XM1 numerical acceptance requires a supported finite x in [-1,1]. FYL2XP1
uses the conservative common interval |x| < 1-sqrt(1/2), inside both Intel's
historic symmetric interval and AMD's wider positive interval. Integer payload
ordering implements these tests, with no C floating point evaluation. For the
two restricted opcodes, nonfinite/unsupported x encodings are conservatively
unclassified. Every such input still participates in memory, alias, CW,
relative TOP, tag, zero-extension and structural-status acceptance. Defined
masked-invalid results for other opcodes retain exact hardware-oracle checks.

B/ES/SF/TOP are always compared. Numeric exception bits are compared only in
classified domains. FSIN/FCOS C2 is compared for supported finite inputs; C1
rounding is compared only with a defined precision outcome and C2 clear.
FPREM/FPREM1 C2 is compared for supported finite operands and a nonzero divisor;
quotient C0/C1/C3 are compared only on completed reduction. Other rounding C1
is gated only on finite operands, a precision exception and no invalid/divide
by zero exception. Other condition bits and final cleanup condition bits are
raw diagnostics, never mandatory equality gates.

The contract is based on the instruction entries and x87 status description in
[Intel SDM](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf)
and [AMD APM](https://docs.amd.com/v/u/en-US/26569-3.15).

All valid PC settings, all four RC settings, and all eight empty-tag physical
TOP prestates are tested. Snapshot TOP is relative to entry TOP; cleanup must
restore entry TOP and empty tags. Inputs include
signed zero, precision beyond F64, both sides of the positive/negative 2^63
trigonometric range limit, subnormals, minimum normals, maximum finite values,
infinities, quiet/signaling NaNs, pseudo-denormals and unsupported encodings.
Every binary operation crosses all original 23 input patterns. Six additional
restricted-domain probes cover +/-1/4 and the adjacent representable values
immediately inside/outside both FYL2XP1 bounds; all four modes, aliases, guards,
PC/RC and TOP prestates apply to those probes for both restricted opcodes. Each operation is also
called with output aliasing either input and with inputs/output terminating
exactly at guarded page boundaries. All four result-liveness modes are checked. Unused outputs additionally receive an inaccessible pointer and must not access it.

The exact finite roster has 288 objects: v1/v2/v3, three selectors, Fast/Full
with normal/tiny/spill pools plus two custom sparse allocation plans sharing
the F80 result with either dying input, raw/optimized, typed/text emission.
Each genuine compiler runs every object: 576 native runs with these exact counters per run:

- 1,503,936 safety/status invocations (185,688 prior cases × eight TOPs, plus 18,432 restricted-domain probes).
- 654,720 mandatory defined-domain payload comparisons.
- 96,384 out-of-domain/unclassified payload diagnostics.
- 1,503,936 optional full raw-status observations.

Payload diagnostics divide into 9,984 F2XM1 and 86,400 FYL2XP1 cases. No
numerical bits inside an undefined ten-byte output range are compared as part
of surrounding memory guards: every byte outside that range is still checked,
including output/input aliases. Unused outputs still require no memory write.
The runner separately records bounded mismatch counters for undefined outputs
and raw status; either counter may be nonzero without weakening defined-bit
acceptance.
Compiler identities, hashes, exact success output and encoding bytes are
required; missing dependencies or unsupported host are errors, never passes.
`--check` validates the sealed packet locally without claiming native execution.
