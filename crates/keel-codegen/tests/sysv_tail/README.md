# SysV tail packet

Export all 24 selector, allocator, register-pool and CPU-level combinations.
The packet covers V1 and V3; V2 emits the same text as V1.

```sh
KEEL_SYSV_TAIL_EXPORT="$PWD/target/sysv-tail" cargo test -p keel-codegen --test x86_sysv_tail
```

On an x86-64 Linux host that supports x86-64-v3, run:

```sh
python3 target/sysv-tail/run.py target/sysv-tail
```

The runner compiles `oracle.c` with GCC and Clang, links each exported object
with `depth.S`, and requires a successful exit and exact pass transcript for
48 executions. A constant-return replacement must fail the value checks once
per compiler. Use `--compiler clang` to select one compiler. Compilation and
byte parity during the Rust test do not establish native value correctness.
The export roster therefore keeps `native_accepted` false.

The C type roster is independent of Keel's classifier. It checks argument
reversal and duplication through direct and indirect tails, aggregate return
protocols, scalar tails, both exhausted register banks, i128 and x87 arguments,
constant stack depth, and sret aliasing. Checks exclude aggregate padding.
The interior-source overlap check only compares words within the first source
object; it makes no claim about adjacent byval storage.
