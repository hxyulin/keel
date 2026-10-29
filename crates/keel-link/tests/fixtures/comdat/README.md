# Real ELF C++ COMDAT fixtures

`template.cpp` is freestanding C++17: a noinline template function, an inline
function referring to an inline variable, and one ordinary wrapper per TU.
Both compilers emit real ELF groups, grouped RELA attachments, section-symbol
references from shared `.eh_frame`, and mixed ordinary/grouped code.
No exception, C++ library, TLS, or GNU-unique runtime support is implied.

Producers:

- `clang-{a,b}.o`: Apple Clang 21.0.0 (clang-2100.3.34.2), cross target
  `--target=x86_64-linux-gnu`.
- `gcc-{a,b}.o`: Debian GCC 14.2.0-19, GNU as 2.44. `-fno-gnu-unique`
  deliberately requests ordinary weak inline-variable binding; native
  `STB_GNU_UNIQUE` is still unsupported.

Reproduce, for UNIT=a and UNIT=b:

```sh
clang++ --target=x86_64-linux-gnu -std=c++17 -O0 -fno-exceptions -fno-rtti \
  -fno-stack-protector -fno-pic -DUNIT=a -c template.cpp -o clang-a.o
x86_64-linux-gnu-g++ -std=c++17 -O0 -fno-exceptions -fno-rtti \
  -fno-stack-protector -fno-pic -fno-gnu-unique -DUNIT=a \
  -S template.cpp -o gcc-a.s
x86_64-linux-gnu-as gcc-a.s -o gcc-a.o
```

`cargo test -p keel-link --test elf elf_comdat` consumes the checked objects without
requiring any external compiler. Other fixtures in that test cover multi-member
atomic selection, local targets and discarded-local diagnosis, signature binding
and section-symbol identities, strong overrides, cross-group references, archives,
lifecycle/unwind pruning, custom retained groups and malformed group structures.

Cross-tool image comparison (no execution by default):

```sh
python3 suite/validate_elf_comdat.py --keel target/debug/keel \
  --gnu-ld x86_64-linux-gnu-ld --lld ld.lld --out target/elf-comdat
```

Validated with GNU ld 2.47.20260726 on macOS (ELF cross-linker) and Debian LLD 19.1.7
in a disposable tooling container. Four cases (each producer, both input orders)
matched selected template instruction bytes, inline-variable bytes, symbol
uniqueness, and four FDEs across Keel/GNU/LLD images. No generated ELF program
was executed for this milestone. `--run` is available only on existing native
x86-64 Linux hosts and must be requested explicitly.

The existing CRT validator now includes the template as a positive O0/O2 case;
that expanded Linux CRT runtime validation has not been run here.
