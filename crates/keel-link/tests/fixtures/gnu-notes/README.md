# GNU metadata fixtures

These are exact section bytes copied from retained inputs and GNU outputs in
`target/elf-system-runtime-20261020/metadata-contract-r1/remote`.
`provenance.json` records original file and extracted-section SHA256 values.
The original report and inputs were read without modification.

The research report dated 2026-10-20 qualifies the tools: the installed
binutils package is **2.47-4**, while `ld`, `as`, and `readelf` binary banners
say **2.46**. The installed SFrame header matches upstream 2.47. The retained
GNU outputs are an independent property oracle, not confirmed upstream 2.47
binary executions and not Keel runtime results.

CRT fixtures cover the observed multiple notes and unsorted USED records.
Tests place these note bytes in small purpose-built relocatables to isolate
metadata behavior. They do not strip CRT objects and do not claim full CRT
linking. ABI bytes test preservation and internal local-symbol relocation
identity; the output symbol table remains global-only.

`split-alone` and `split-all` check per-object duplicate reduction before the
explicit surviving-IBT rejection. `split-missing`, `split-usedzero`, and
`isazero-gnu` are compared byte-for-byte with Keel's canonical property note.
These references come from the report's GNU-as fixtures using
`-mx86-used-note=no`; the fixtures and expected outputs were not regenerated
with a different toolchain.

The implementation contract follows retained primary-source snapshots in that
report's `sources/` directory:

- Pinned binutils `binutils-2_47` `bfd/elfxx-x86.c`:
  `_bfd_x86_elf_parse_gnu_properties` ORs duplicate values per BFD;
  `_bfd_x86_elf_merge_gnu_properties` supplies AND/OR/OR_AND rules.
- Linux ABI `object-files.tex`: note/property formats and note alignment.
- x86-64 psABI `dl.tex`: masks, presence rules and basic-x86 FEATURE_2_USED.
- glibc `sysdeps/x86/dl-prop.h` and Linux `fs/binfmt_elf.c`: loader-visible
  property discovery, bounds and canonical ordering.

The checked psABI master was `e1ce098331da5dbd66e1ffc74162380bcc213236`.
The report retains source digests and distinguishes producer compatibility
from the stricter canonical output contract.

`libreview.so` is copied unchanged from the independent review's `review-r1/`
evidence. Its `make-dso.py` constructs a complete ELF64 DSO with two LOADs,
DYNAMIC, SONAME `libreview.so`, SysV hash and a writable R_X86_64_64 relocation
to strong undefined OBJECT `prop`. It needs no compiler or host execution.
Tests inspect this interface and mutate binding only for weak lookup controls.
DSO SHA256: `98c3ecd3dfd0ee9f6f9e2c283e1d807ec1049101324b5c9435fe0f81c6e62af5`.
Generator SHA256: `f2849bd83fd612be95c6d100c4b8a029ac735c5b2c0af3ef7d1b405217bf34dc`.

`libdefined.so`, `libweak-defined.so`, `libprotected-defined.so` and
`libundefined.so`, with their four C sources, are unchanged copies from the
independent `review-r2` evidence. `compiler-provenance.json` records whole-file
hashes and original locations. Apple Clang 21 compiled these with
`-target x86_64-unknown-linux-gnu -fPIC -fno-asynchronous-unwind-tables
-fno-stack-protector -c NAME.c -o NAME.o`. Rust LLD 23.1.1 linked them with
`-flavor gnu -shared --hash-style=sysv -soname libNAME.so NAME.o -o libNAME.so`.
The sources use ordinary pointer initializers. Defined GLOBAL/WEAK inputs have
real named R_X86_64_64 dynamic lookups; the protected control uses RELATIVE.
Tests consume retained ELF bytes without requiring the cross compiler or LLD.
These are structural preemption controls, not native Linux loader executions.

`got-defined.c`/`libgot-defined.so` and `plt-defined.c`/`libplt-defined.so` were
built with the same local tools/flags during fixes-r2; exact commands and hashes
are in `compiler-provenance.json`. They exercise real GLOB_DAT and JUMP_SLOT
lookups. The PLT fixture also has an independent R_X86_64_64 relocation against
`other` in `.rela.dyn`, so the test requires checking both runtime tables.
