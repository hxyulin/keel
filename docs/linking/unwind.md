# Unwind metadata

Keel's native backends emit call-frame information (CFI) by default for every
function. The `.cfi_*` assembly directives describe how to recover the caller's
stack and saved registers. They are metadata directives, not executed unwind
instructions. On Linux, the assembler turns them into `.eh_frame` records and
relocations. On Darwin, it can also produce compact-unwind records.

This used to be unconditional. Neither `-O0` nor `-Os` selected an unwind policy.
The default remains unchanged; `cc` and `compile` now accept explicit controls:

```sh
keel cc --target x86_64-unknown-linux-gnu -Os \
  -fno-asynchronous-unwind-tables input.c --emit-object -o input.o
```

The compiler has independent synchronous and asynchronous table requests.
Synchronous tables default off; asynchronous tables default on. Repeating a flag
changes only its own request, with the last value winning. Either enabled
request emits Keel's existing CFI. This does not introduce a new unwind format
or change the existing precision of the generated frame rules.

| Flags | Optional compiler-generated CFI |
|---|---|
| None | Emitted. |
| `-fno-asynchronous-unwind-tables` | Omitted. |
| `-fno-unwind-tables` | Emitted because the asynchronous request remains enabled. |
| `-fno-asynchronous-unwind-tables -funwind-tables` | Emitted. |
| `-funwind-tables -fno-asynchronous-unwind-tables` | Emitted. |
| `-fno-unwind-tables -fno-asynchronous-unwind-tables` | Omitted. |

This distinction agrees with the local Clang x86-64 Linux oracle. GCC documents
both requests in its [code generation options](https://gcc.gnu.org/onlinedocs/gcc/Code-Gen-Options.html).
For a build that wants neither request, pass both negative options. They are
independent of optimization level, CPU features, address models and `-g`.
Wasm retains its separate output contract and rejects these native flags.

## What omission means

The flag gates compiler-created CFI during emission. It leaves instruction
selection, allocation, stack frames, callee-saved preservation and function
alignment unchanged. Tests compare identical `.text` bytes with tables on and
off. Debug line and variable information remain available with `-g`.

CFI written explicitly in inline/file-scope assembly is preserved. The assembler
still processes that CFI normally, so a file can still contain `.eh_frame` even
when optional compiler tables are disabled. Standalone `keel asm` behavior is
unchanged. Raw CFI that relies on an enclosing compiler-created CFI procedure
must supply a valid procedure when that compiler procedure is disabled.

A function with an exception-handling personality retains its required CFI and
LSDA association. Current native exception handling is supported on AArch64
Darwin; Linux's existing exception-handling rejections remain unchanged. Removing
ordinary frame tables means callers must not rely on unwinding or stack traces
through those ordinary frames. Omission does not itself create a kernel-specific
unwinder, ORC tables or frame-pointer policy.

## Library API

`UnwindTables::All` is the default. `UnwindTables::RequiredOnly` suppresses
optional compiler-created metadata. `EmissionOptions` combines this policy with
an optional x86 address model, independently of `CodegenPipeline`.

Use `compile_with_pipeline_and_emission`,
`compile_program_with_pipeline_and_emission`, or
`compile_program_chunks_with_pipeline_and_emission`. Existing APIs retain their
previous defaults. Cached AArch64 leaf bodies use the same policy as ordinary
functions. The policy is scoped to one call; it uses no process-global state.

Tests cover source/typed/chunked object parity on both Linux targets and Darwin,
all stable selectors/allocators, explicit assembly CFI, independent flag ordering,
`-g`, kernel-address relocations, and native C++ exception execution through
required Darwin frames. The full pinned kernel build/boot qualification has not
been repeated with these flags.

Validation on 2026-10-28: the full workspace run passed 3,001 tests with zero
failures and 15 ignored tests. The experimental-allocator configuration checked
successfully. Scoped formatting and documentation generation passed. Ordinary
Clippy completed with existing warnings; warnings-denied Clippy remains blocked
by existing linker diagnostics.
