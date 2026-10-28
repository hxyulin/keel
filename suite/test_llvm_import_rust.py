#!/usr/bin/env python3
"""Run an existing LLVM import validator against Rust and its Python reference.

Every importer CLI invocation runs both implementations and compares acceptance
and successful output. The validator receives the Rust result and executes its
usual native interoperability and rejection checks. Artifacts stay in --out.
"""
import argparse
import pathlib
import runpy
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--importer', required=True, type=pathlib.Path)
    parser.add_argument('--validator', required=True,
                        choices=['base', 'flags', 'fp', 'globals', 'calls', 'bool',
                                 'memory', 'aggregate', 'fast', 'eh', 'linkage', 'odr', 'policy'])
    args, remaining = parser.parse_known_args()
    suffix = '' if args.validator == 'base' else '_' + args.validator
    validator = (ROOT / 'suite/test_llvm_import_odr.py' if args.validator == 'policy'
                 else ROOT / f'suite/llvm_import{suffix}_validate.py')
    original_run = subprocess.run
    count = 0

    def run(command, *positional, **kwargs):
        nonlocal count
        argv = list(map(str, command)) if isinstance(command, (list, tuple)) else []
        if len(argv) < 2 or pathlib.Path(argv[1]).resolve() != ROOT / 'suite/llvm_import.py':
            return original_run(command, *positional, **kwargs)
        # Preserve the reference destination before invoking either CLI.
        dest = None
        for flag in ('-o', '--output'):
            if flag in argv:
                value = argv[argv.index(flag) + 1]
                if value != '-':
                    dest = pathlib.Path(value)
        before = dest.read_bytes() if dest is not None and dest.exists() else None
        python = original_run(command, *positional, **kwargs)
        reference = dest.read_bytes() if dest is not None and dest.exists() else None
        if dest is not None:
            if before is None:
                dest.unlink(missing_ok=True)
            else:
                dest.write_bytes(before)
        rust = original_run([str(args.importer.resolve()), *argv[2:]], *positional, **kwargs)
        assert (python.returncode == 0) == (rust.returncode == 0), (
            argv, python.returncode, python.stderr, rust.returncode, rust.stderr)
        if python.returncode == 0:
            assert python.stdout == rust.stdout, (argv, 'stdout differs')
            actual = dest.read_bytes() if dest is not None and dest.exists() else None
            assert reference == actual, (argv, 'Keel output differs')
        elif dest is not None:
            actual = dest.read_bytes() if dest.exists() else None
            assert actual == before, (argv, 'Rust failure changed destination')
        count += 1
        return rust

    if args.validator == 'policy':
        import llvm_import
        reference_import = llvm_import.import_module

        def import_module(llvm, source, localize_odr=False):
            nonlocal count
            cmd = [str(args.importer.resolve()), '--libllvm', remaining[remaining.index('--libllvm') + 1], '-']
            if localize_odr:
                cmd.append('--localize-odr-functions')
            rust = original_run(cmd, input=source, capture_output=True, timeout=30)
            try:
                python = reference_import(llvm, source, localize_odr=localize_odr)
            except llvm_import.Unsupported:
                assert rust.returncode != 0, ('Rust accepted rejected input', source, rust.stdout)
                count += 1
                raise
            assert rust.returncode == 0, (source, rust.stderr)
            assert python.encode() == rust.stdout, ('Keel output differs', source, python, rust.stdout)
            count += 1
            return rust.stdout.decode()

        llvm_import.import_module = import_module
    subprocess.run = run
    sys.argv = [str(validator), *remaining]
    try:
        runpy.run_path(str(validator), run_name='__main__')
    finally:
        print(f'Rust/Python importer CLI comparisons: {count}', file=sys.stderr)


if __name__ == '__main__':
    main()
