#!/usr/bin/env python3
"""Representative C library compatibility check; run under the suite host lock.

Requires a built keel CLI and Clang (reference compiler and system linker).
All generated files are placed in the required, caller-private --out-dir.
"""
import argparse
import pathlib
import platform
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keel', type=pathlib.Path, required=True)
    parser.add_argument('--out-dir', type=pathlib.Path, required=True)
    args = parser.parse_args()
    keel = str(args.keel.resolve())
    out = args.out_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    source = pathlib.Path(__file__).with_name('attribute-library.c').resolve()

    def checked(argv):
        result = subprocess.run(list(map(str, argv)), capture_output=True, text=True)
        assert result.returncode == 0, (argv, result.stdout, result.stderr)
        return result

    reference = out / 'reference'
    checked(['clang', '-std=c11', '-Wno-deprecated-declarations', source, '-o', reference])
    expected = checked([reference])
    preprocessed = out / 'attribute-library.i'
    checked(['clang', '-E', source, '-o', preprocessed])
    targets = ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']
    host = ('aarch64-apple-darwin' if platform.system() == 'Darwin' and platform.machine() == 'arm64'
            else 'x86_64-unknown-linux-gnu' if platform.system() == 'Linux' and platform.machine() == 'x86_64'
            else None)
    executions = 0
    for target in targets:
        for input_path in [source, preprocessed]:
            for optimized in [False, True]:
                obj = out / f'{target}-{input_path.suffix[1:]}-{int(optimized)}.o'
                checked([keel, 'cc', '--target', target, '--emit-object',
                         *(['-O'] if optimized else []), input_path, '-o', obj])
                assert obj.stat().st_size > 0
                if target == host:
                    executable = obj.with_suffix('.exe')
                    checked(['clang', obj, '-o', executable])
                    actual = checked([executable])
                    assert (actual.stdout, actual.stderr) == (expected.stdout, expected.stderr)
                    executions += 1

    rejected = [
        ('weak', 'int hook(void) __attribute__((weak));'),
        ('alias', 'int implementation(void) { return 1; } int hook(void) __attribute__((alias("implementation")));'),
        ('constructor', 'static void init(void) __attribute__((constructor)); static void init(void) {}'),
        ('cleanup', 'void release(int *); int f(void) { int x __attribute__((cleanup(release))) = 0; return x; }'),
        ('visibility', 'int hook(void); int hook(void) __attribute__((visibility("hidden")));'),
    ]
    for target in targets:
        for name, text in rejected:
            path = out / f'{name}.c'
            path.write_text(text + '\n')
            obj = out / f'rejected-{target}-{name}.o'
            obj.unlink(missing_ok=True)
            result = subprocess.run([keel, 'cc', '--target', target, '--emit-object', str(path), '-o', str(obj)], capture_output=True, text=True)
            assert result.returncode != 0 and name in result.stderr, result
            assert not obj.exists(), obj
    print(f'PASS: 8 objects; {executions} native executions matched Clang; 10 semantic-attribute CLI rejections')


if __name__ == '__main__':
    main()
