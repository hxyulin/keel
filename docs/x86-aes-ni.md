# Legacy 128-bit AES-NI

Keel supports the six explicit legacy AES-NI operations on
`x86_64-unknown-linux-gnu`. Enable the independent `aes` feature with
`--target-features +aes` for `asm`, `compile` or `cc`, or
`Features::for_level(level).with_features("+aes")` through `FeatureSelector`.
It is disabled in the historical default and strict v1/v2/v3 profiles.
`-aes` disables it again. `+aes` does not enable AVX or any other feature;
PCLMULQDQ has a separate feature contract. No fallback bypasses disabled AES.
The deployment CPU must support CPUID.01H:ECX.AES[25] and the normal OS-enabled
XMM state contract. Keel does not insert a runtime probe or dispatch.

## Architectural operations

| IR intrinsic | Parameters → result | Legacy instruction |
|---|---|---|
| `x86_aesenc128` | `i64x2, i64x2 → i64x2` | `AESENC` |
| `x86_aesenclast128` | `i64x2, i64x2 → i64x2` | `AESENCLAST` |
| `x86_aesdec128` | `i64x2, i64x2 → i64x2` | `AESDEC` |
| `x86_aesdeclast128` | `i64x2, i64x2 → i64x2` | `AESDECLAST` |
| `x86_aesimc128` | `i64x2 → i64x2` | `AESIMC` |
| `x86_aeskeygenassist128` | `i64x2, i32 → i64x2` | `AESKEYGENASSIST` |

The integer vector is a 128-bit container, with byte zero at the lowest address.
Rounds consume the state first and round key second. Encryption applies
ShiftRows, SubBytes, MixColumns and AddRoundKey; the last round omits
MixColumns. Decryption uses the Equivalent Inverse Cipher: InvShiftRows,
InvSubBytes, InvMixColumns and AddRoundKey; its last round omits InvMixColumns.
AESIMC applies InvMixColumns to its input. These are pure bit transformations;
they do not access implicit memory or alter MXCSR.

AESKEYGENASSIST requires an `iconst.i32` in 0..255, checked by the IR verifier
and backend preflight. If the four input little-endian doublewords are
`X0,X1,X2,X3`, its output doublewords from low to high are
`SubWord(X1), RotWord(SubWord(X1)) XOR imm8, SubWord(X3),
RotWord(SubWord(X3)) XOR imm8`. RotWord is a 32-bit rotate right by eight;
the immediate is zero-extended into the low byte of its doubleword.

The semantic authority is Intel's
[AES New Instructions Set white paper](https://www.intel.com/content/dam/doc/white-paper/advanced-encryption-standard-new-instructions-set-paper.pdf),
particularly the instruction pseudocode and byte/doubleword ordering on
pages 15–19. Algorithm and known-answer vectors follow
[NIST FIPS 197](https://nvlpubs.nist.gov/nistpubs/fips/nist.fips.197.pdf),
including Appendix C.1's AES-128 encryption and decryption example.

The typed assembler accepts legacy XMM register sources and m128 sources,
including extended registers and supported ordinary addressing/segment forms.
Round destinations are destructive XMM operands; AESIMC and AESKEYGENASSIST
replace their destination. Legacy memory sources access exactly 16 bytes and
require 16-byte alignment. Register intrinsics retain explicit loads, including
unaligned loads, rather than folding them into an alignment-requiring source. Mandatory
66, REX extension bits, 0F38 DB–DF and 0F3A DF/imm8 use the existing typed
legacy table and encoder. Wrong register classes, explicit memory widths and
invalid immediate controls diagnose. IR lowering stages inputs in reserved
scratch XMM registers before writing the destination, handling live values,
coalesced operands and spills without changing the existing ABI or load/store
footprints. Both assembler and ordinary emission feature guards apply to text,
typed and chunked objects, in debug and release. ARM backends reject these IR
operations.

## Bounded C surface

`<wmmintrin.h>` and `<immintrin.h>` expose these six wrappers using the existing
`__m128i` container:

- `_mm_aesenc_si128(a,b)`, `_mm_aesenclast_si128(a,b)`
- `_mm_aesdec_si128(a,b)`, `_mm_aesdeclast_si128(a,b)`
- `_mm_aesimc_si128(a)`
- `_mm_aeskeygenassist_si128(a,control)`

The AES-owned definitions live in `<__keel_aes.h>`, a fragment of the shared
`wmmintrin.h` surface. Each macro evaluates each vector operand once and maps
to the corresponding checked `__builtin_ia32_<instruction>128` builtin.
Operands must have the matching signed long-long 16-byte vector type used by
`__m128i`; mixed vector types diagnose. AESKEYGENASSIST's control must be an
integer constant expression in 0..255, within the frontend’s bounded
64-bit constant evaluator (128-bit/pointer intermediates diagnose). The same reviewed bounded immediate proof used by PCLMUL rejects invalid
shift counts, signed overflow, unrepresentable float casts and non-ICE floating
expressions before folding. The parser canonicalizes it to the
required IR constant; dynamic, floating and out-of-range controls diagnose.
Header inclusion enables no features and emits no unused functions.
`__has_builtin` reports frontend recognition on x86, independently of the CPU
feature selection; it returns false on ARM, where direct builtin use and header
inclusion diagnose.

This contract adds no VEX/EVEX/VAES, AVX-512, automatic AES recognition,
per-function target dispatch, AES modes, GCM or crypto library. It makes no
whole-program constant-time or security claim.

## Finite validation

`keel-codegen --test x86_aes` covers all six operations with an independent
GF(256) software oracle (algebraic S-box construction, inverse rows and column
transforms), the FIPS AES-128 encryption/decryption vector, 128 basis cases,
zero/all-one edges and deterministic random inputs. All 256 key-assist controls
are checked. Native execution is guarded by AES/SSE2 detection in Rust and C.
All three selection tiers, both allocators, normal and empty register pools
are exercised. Tests keep both inputs live, use identical operands, preserve
input bytes, check direct/indirect vector calls with SIMD-clobbering callbacks,
MXCSR and output sentinels, and test both guard-page edges. Separate assembled
m128-source kernels check the assembler's exact 16-byte read/store footprints.

The encoding matrix compares text bytes with independent Clang and, when
`KEEL_GNU_AS` is set, GNU `as` objects, including extended registers,
base/index/scale/displacements, RIP-relative and FS memory forms, and immediate
boundaries. Typed formatting/parsing, AT&T source parsing and text/typed/chunked
codegen object parity are checked. CLI and C tests check strict-level rejection,
explicit acceptance, frontend recognition, ICE/type/foreign diagnostics,
unused-header behavior and single evaluation. The C native wrapper test also
executes all six wrappers and all 256 key-assist controls against the same
independent oracle, including a side-effecting vector source. The existing ordinary emission
guard test includes AES and is run in debug and release.

Run serialized, using the existing toolchain and per-checkout managed targets:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-aes-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen --test x86_aes
python3 suite/fast_opt_locked.py /tmp/keel-aes-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-c --test x86_aes --test x86_intrinsics
python3 suite/fast_opt_locked.py /tmp/keel-aes-unique.jsonl \
  cargo test --offline --locked -j2 -p keel --test x86_aes
python3 suite/fast_opt_locked.py /tmp/keel-aes-unique.jsonl \
  cargo test --offline --locked -j2 --release -p keel-codegen --lib feature_guard_tests
```

On 2026-10-23, final validation after rebasing onto main `a55b8539` passed
all 12 AES/PCLMUL C/codegen tests in both debug and release on LinuxHost
(Ryzen 9 9950X3D), with both Clang and GCC harness compilers. The AES backend
campaign covers 12 selection/allocation/pool combinations and two harness
compilers; the C campaign adds baseline/normal and optimized/forced-spill paths.
The 280-case encoding matrix matched both Clang and GNU `as`. Local CLI,
C/header regressions and ordinary debug/release emission guards passed.
Native logs are `/tmp/keel-aes-final-native-{debug,release}.log` on LinuxHost;
the isolated snapshot is `/tmp/keel-aes-20261023.KlX8gG`. Local ARM macOS runs cross-emit x86 ELF objects and exercise encoding,
frontend and feature diagnostics; they do not establish native x86 execution.
Integration preserves PCLMUL intrinsic ID 767 and appends AES IDs 768–773
(774 registry entries). The `pclmulqdq` feature and `pclmul` alias, existing
PCLMUL wrapper and checked immediate restrictions remain independent. Feature
matrices cover neither, AES only, PCLMUL only and both at strict v1/v2/v3.

These are finite tests on an AES-capable host, not exhaustive ISA certification
or execution on a CPU that lacks AES. Generated artifacts and logs stay outside
Git.

Independent review on 2026-10-23 rebased this branch onto main `7cbb9ce6`,
including its owned vector-legalization change. It found and fixed missing
backend operand-shape preflight: unverified IR could supply an i64 key-assist
control or malformed AES arity/types. A regression first reproduced acceptance
of the i64 control and now requires diagnostics. The review also replaced the
native callback's empty assembly clobber declaration with physical XMM1–XMM15
clears, added unaligned inputs to the register-intrinsic execution checks, and
compared symbolic RIP-relative relocations for all six instructions against
Clang and GNU `as` (including the key-assist immediate's displacement addend).
Fresh LinuxHost debug/release validation passed all 14 AES/PCLMUL C/codegen tests
and four owned-vector regression controls. Both harness compilers and all
selection/allocation/pool combinations passed the strengthened AES checks.
Local CLI/C/PCLMUL regressions and debug/release ordinary emission guards passed.
Commands are recorded in `/tmp/keel-aes-review-unique.jsonl` on each host;
native results are `/tmp/keel-aes-review-final-{debug,release}.log` on LinuxHost,
using snapshot `/tmp/keel-aes-review.J9ZjcM`. A separate finite memory probe in
`/tmp/keel-aes-review-memory.log` verified both valid page edges, alignment and
truncated-operand faults, and unaligned register staging for each instruction.
The truncated pointers were also unaligned, so those faults alone do not
distinguish alignment checks from full-width page accesses.
