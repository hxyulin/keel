#!/usr/bin/env python3
"""Aggregate frontend correctness checks. Owns the shared suite lock; do not nest wrappers.

Normally invoked through cargo suite test-frontends; see suite/README.md.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
LOCK = '/tmp/keel-perf-20261019.lock'


def llvm_host(system, machine):
    return system == 'Darwin' and machine in ('arm64', 'aarch64')


class Harness:
    def __init__(self, out):
        self.out = out
        self.stages = []
        self.env = dict(os.environ, CARGO_TARGET_DIR=str(out / 'target'))

    def run(self, name, argv, timeout=900):
        record = {'name': name, 'argv': list(map(str, argv)), 'status': 'failed'}
        self.stages.append(record)
        prefix = self.out / name
        record.update(stdout=str(prefix) + '.stdout', stderr=str(prefix) + '.stderr')
        print(f'frontend checks: {name}', flush=True)
        try:
            result = subprocess.run(record['argv'], cwd=ROOT, env=self.env,
                                    capture_output=True, timeout=timeout)
            stdout, stderr = result.stdout, result.stderr
            record['returncode'] = result.returncode
            if result.returncode == 0:
                record['status'] = 'passed'
        except subprocess.TimeoutExpired as error:
            stdout, stderr = error.stdout or b'', error.stderr or b''
            record['error'] = 'timeout'
        except OSError as error:
            stdout, stderr = b'', str(error).encode()
            record['error'] = 'unavailable'
        Path(record['stdout']).write_bytes(stdout)
        Path(record['stderr']).write_bytes(stderr)
        if record['status'] != 'passed':
            print(f"FAILED: {name}; see {record['stderr']}", file=sys.stderr)
        return record['status'] == 'passed'

    def skip(self, name, reason):
        self.stages.append(dict(name=name, status='skipped', reason=reason))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--keel', type=Path)
    parser.add_argument('--libllvm', type=Path)
    parser.add_argument('--jobs', type=int, default=2)
    parser.add_argument('--require-llvm', action='store_true')
    args = parser.parse_args(argv)
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    out = args.out.resolve()
    try:
        out.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        parser.error('--out must not already exist')
    harness = Harness(out)
    report = {'schema_version': 1, 'status': 'running', 'stages': harness.stages,
              'host': {'system': platform.system(), 'machine': platform.machine()},
              'scope': 'frontend correctness; no performance measurements', 'lock': LOCK}
    cargo = os.environ.get('CARGO', 'cargo')
    cargo_flags = ['--offline', '--locked', '-j', str(args.jobs)]
    code = 1
    try:
        # No child runner acquires this lock. Builds and all native execution
        # share the same lock as the existing standalone validation workflows.
        with open(LOCK, 'a+') as lock:
            print('frontend checks: waiting for shared native-test lock', flush=True)
            fcntl.flock(lock, fcntl.LOCK_EX)
            if harness.run('revision', ['git', 'rev-parse', 'HEAD']):
                report['revision'] = (out / 'revision.stdout').read_text().strip()
            harness.run('working-tree', ['git', 'status', '--porcelain'])
            harness.run('harness-tests', [sys.executable, '-m', 'unittest',
                                        'suite.test_cpp_coverage', 'suite.test_frontend_validate', '-v'])
            harness.run('c-attributes', [cargo, 'test', *cargo_flags, '-p', 'keel-c',
                                       '--test', 'attribute_semantics'])
            harness.run('c-preprocess', [cargo, 'test', *cargo_flags, '-p', 'keel',
                                       '--test', 'preprocess'])
            keel = args.keel.resolve() if args.keel else out / 'target/debug/keel'
            built = bool(args.keel) or harness.run('build-keel', [cargo, 'build', *cargo_flags,
                                                                '-p', 'keel'])
            ready = built and keel.is_file() and os.access(keel, os.X_OK)
            if ready:
                report['keel'] = {'path': str(keel), 'sha256': hashlib.sha256(keel.read_bytes()).hexdigest()}
                harness.run('c-native', [sys.executable,
                    ROOT / 'crates/keel-c/tests/fixtures/check_attribute_semantics.py',
                    '--keel', keel, '--out-dir', out / 'c-native'])
            else:
                harness.stages.append(dict(name='keel-prerequisite', status='failed',
                                           reason='Keel build failed or binary is unavailable'))
                harness.skip('c-native', 'Keel binary unavailable')
            supported = llvm_host(platform.system(), platform.machine())
            libllvm = args.libllvm.resolve() if args.libllvm else None
            if supported and libllvm is None and harness.run('rust-sysroot', ['rustc', '--print', 'sysroot']):
                libllvm = Path((out / 'rust-sysroot.stdout').read_text().strip()) / 'lib/libLLVM.dylib'
            if supported and ready and libllvm is not None:
                report['libllvm'] = str(libllvm)
                harness.run('llvm-import', [sys.executable, ROOT / 'suite/llvm_import_validate.py',
                    '--libllvm', libllvm, '--keel', keel, '--out', out / 'llvm-import'])
                harness.run('llvm-calls', [sys.executable, ROOT / 'suite/llvm_import_calls_validate.py',
                    '--libllvm', libllvm, '--keel', keel, '--out', out / 'llvm-calls'])
                harness.run('llvm-fp', [sys.executable, ROOT / 'suite/llvm_import_fp_validate.py',
                    '--libllvm', libllvm, '--keel', keel, '--out', out / 'llvm-fp'])
                harness.run('llvm-globals', [sys.executable, ROOT / 'suite/llvm_import_globals_validate.py',
                    '--libllvm', libllvm, '--keel', keel, '--out', out / 'llvm-globals'])
                harness.run('llvm-linkage', [sys.executable, ROOT / 'suite/llvm_import_linkage_validate.py',
                    '--libllvm', libllvm, '--keel', keel, '--out', out / 'llvm-linkage'])
                harness.run('llvm-flags', [sys.executable, ROOT / 'suite/llvm_import_flags_validate.py',
                    '--libllvm', libllvm, '--keel', keel, '--out', out / 'llvm-flags'])
                harness.run('llvm-eh', [sys.executable, ROOT / 'suite/llvm_import_eh_validate.py',
                    '--libllvm', libllvm, '--keel', keel, '--out', out / 'llvm-eh'])
                harness.run('odr-core', [cargo, 'test', *cargo_flags, '-p', 'keel-ir',
                    '-p', 'keel-opt', '-p', 'keel-codegen', '-p', 'keel-llvm', '--test', 'weak_odr'])
                harness.run('llvm-odr-policy', [sys.executable, ROOT / 'suite/test_llvm_import_odr.py',
                    '--libllvm', libllvm, '--keel', keel, '--verify-weak-odr', '-v'])
                harness.run('llvm-odr', [sys.executable, ROOT / 'suite/llvm_import_odr_validate.py',
                    '--libllvm', libllvm, '--keel', keel, '--out', out / 'llvm-odr'])
                harness.run('cpp-coverage', [sys.executable, ROOT / 'suite/cpp_coverage.py',
                    '--libllvm', libllvm, '--keel', keel, '--abi-rustc', 'rustc',
                    '--out', out / 'cpp-coverage'])
                # These fixtures export only cpp_case; their ODR helper symbols
                # are not used by the separately compiled C/Rust boundaries.
                harness.run('cpp-odr', [sys.executable, ROOT / 'suite/cpp_coverage.py',
                    '--libllvm', libllvm, '--keel', keel, '--abi-rustc', 'rustc',
                    '--localize-odr-functions', '--case', 'constexpr_runtime',
                    '--out', out / 'cpp-odr'])
            else:
                reason = ('LLVM import currently requires Apple AArch64' if not supported
                          else 'LLVM library discovery or Keel prerequisite failed')
                for name in ('llvm-import', 'llvm-calls', 'llvm-fp', 'llvm-globals', 'llvm-linkage',
                             'llvm-flags', 'llvm-eh', 'odr-core', 'llvm-odr-policy', 'llvm-odr',
                             'cpp-coverage', 'cpp-odr'):
                    harness.skip(name, reason)
                if supported or args.require_llvm:
                    harness.stages.append(dict(name='llvm-prerequisite', status='failed', reason=reason))
            failed = any(stage['status'] == 'failed' for stage in harness.stages)
            skipped = any(stage['status'] == 'skipped' for stage in harness.stages)
            report['status'] = 'failed' if failed else ('passed_with_skips' if skipped else 'passed')
            code = int(failed)
    finally:
        if code:
            report['status'] = 'failed'
        report['exit_code'] = code
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'status': report['status'], 'report': str(out / 'report.json'),
                          'exit_code': code}), flush=True)
    return code


if __name__ == '__main__':
    raise SystemExit(main())
