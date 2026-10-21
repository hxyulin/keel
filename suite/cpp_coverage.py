#!/usr/bin/env python3
"""Clang C++ -> untouched LLVM IR -> llvm_import -> native differential checks.

Run under suite/fast_opt_locked.py; see cpp_coverage_README.md. No timing claims.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'suite/cpp_coverage_fixtures'
IMPORTER = ROOT / 'suite/llvm_import.py'
CASES = [
    # Required profiles establish a regression floor; probes can evolve to pass.
    ('scalar', 'scalar wrapping arithmetic and volatile memory', True, '-O2'),
    ('templates', 'inlined template instantiation', True, '-O2'),
    ('constexpr', 'compile-time evaluation', True, '-O2'),
    ('overloads', 'overload resolution and mangled scalar definitions', True, '-O2'),
    ('raii', 'inlined destructor with observable C callback and volatile store', True, '-O2'),
    ('c_abi', 'cross-object C -> C++ -> C calls', True, '-O2'),
    ('constexpr_runtime', 'constexpr helper invoked outside a constant-expression context', False, '-O2'),
    ('virtual', 'virtual dispatch, vtable/global/indirect-call machinery', False, '-O2'),
    ('cfg', 'multiple blocks and conditional direct calls', True, '-O1'),
    ('gep', 'indexed volatile memory', True, '-O1'),
    ('alloca', 'unoptimized stack locals', True, '-O0'),
    ('signed_contract', 'compiler arithmetic no-wrap contracts', False, '-O1'),
    ('eh', 'throw/catch and exception runtime', False, '-O1'),
]
SENTINEL = b'cpp-coverage destination preservation sentinel\n'


class Failed(Exception):
    pass


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def classify_import(returncode, stdout, stderr, preserved):
    """Only known unsupported diagnostics count as coverage blockers."""
    if returncode == 0:
        return 'accepted' if not stdout and not preserved else 'import_protocol_failure'
    if not preserved or stdout:
        return 'destination_preservation_failure'
    if returncode is None:
        return 'import_timeout_or_unavailable'
    if returncode != 1:
        return 'import_crash' if returncode < 0 else 'import_failure'
    # Parser errors, Keel verifier failures, missing APIs and library failures
    # must not be hidden as expected feature rejections.
    diagnostic = stderr.removeprefix('llvm-import: ').strip()
    known = ('unsupported', 'not supported', 'single-block', 'multiple blocks',
             'only single', 'global data', 'globals', 'nsw', 'nuw', 'poison',
             'ordinary C', 'supported types', 'personality', 'intrinsic',
             'stack allocation', 'non-C calling convention', 'variadic signature',
             'i1 ABI', 'no-wrap', 'flagged')
    if (stderr.startswith('llvm-import: ') and
            'Keel verification failed' not in diagnostic and
            not re.search(r'input\.ll:\d+:\d+:', diagnostic) and
            any(word in diagnostic for word in known)):
        return 'rejected'
    return 'import_failure'


class Runner:
    def __init__(self, out, timeout):
        self.out, self.timeout, self.commands = out, timeout, []

    def run(self, command, cwd=None):
        argv = list(map(str, command))
        index = len(self.commands)
        record = {'argv': argv, 'cwd': str(cwd or ROOT)}
        self.commands.append(record)
        prefix = self.out / f'command-{index:04d}'
        record.update(stdout=str(prefix) + '.stdout', stderr=str(prefix) + '.stderr')
        try:
            result = subprocess.run(argv, cwd=cwd or ROOT, capture_output=True,
                                    timeout=self.timeout)
            stdout, stderr = result.stdout, result.stderr
            record['returncode'] = result.returncode
        except subprocess.TimeoutExpired as e:
            stdout, stderr = e.stdout or b'', e.stderr or b''
            record.update(returncode=None, failure='timeout')
        except OSError as e:
            stdout, stderr = b'', str(e).encode()
            record.update(returncode=None, failure='unavailable')
        Path(record['stdout']).write_bytes(stdout)
        Path(record['stderr']).write_bytes(stderr)
        return record

    def require(self, command):
        result = self.run(command)
        if result['returncode'] != 0:
            raise Failed(f"command failed; see {result['stderr']}")
        return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--libllvm', required=True)
    parser.add_argument('--keel', required=True)
    parser.add_argument('--out', required=True, help='new private artifact directory; never overwritten')
    parser.add_argument('--clangxx', default='clang++')
    parser.add_argument('--clang', default='clang')
    parser.add_argument('--abi-cc', help='also test a separately compiled C boundary with this compiler (e.g. gcc-15)')
    parser.add_argument('--abi-rustc', help='also test the independent no_std Rust C-ABI boundary (e.g. rustc)')
    parser.add_argument('--target', help='explicit Clang target; default is recorded compiler host target')
    parser.add_argument('--case', action='append', choices=[c[0] for c in CASES], dest='cases')
    parser.add_argument('--timeout', type=float, default=90)
    args = parser.parse_args(argv)
    out = Path(args.out).resolve()
    try:
        out.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        parser.error('--out must not already exist (artifacts are preserved)')
    runner = Runner(out, args.timeout)
    report = {'schema_version': 1, 'status': 'running', 'cases': [],
              'commands': runner.commands, 'root': str(ROOT), 'artifacts': str(out),
              'scope': 'C++ via Clang-prepared LLVM IR; no native C++ frontend or performance claim',
              'tiers': ['none', 'fast', 'full'], 'input_pairs': 4240,
              'environment': {}, 'sources': {}}
    exit_code = 1
    try:
        # Preserve source snapshots as well as their identities for reruns.
        shutil.copytree(FIXTURES, out / 'sources')
        for source in sorted((out / 'sources').iterdir()):
            report['sources'][source.name] = digest(source)
        for key, tool in [('clangxx', args.clangxx), ('clang', args.clang), ('abi_cc', args.abi_cc), ('abi_rustc', args.abi_rustc)]:
            if tool:
                record = runner.require([tool, '--version'])
                report['environment'][key] = {
                    'path': shutil.which(tool) or tool,
                    'version': Path(record['stdout']).read_text(errors='replace')}
        revision = runner.require(['git', 'rev-parse', 'HEAD'])
        report['environment']['git_commit'] = Path(revision['stdout']).read_text().strip()
        shutil.copy2(IMPORTER, out/'llvm_import.py')
        report['environment']['python'] = sys.version
        report['environment']['keel'] = {'path': str(Path(args.keel).resolve()), 'sha256': digest(Path(args.keel))}
        report['environment']['importer_sha256'] = digest(IMPORTER)
        report['environment']['libllvm'] = str(Path(args.libllvm).resolve())
        probe = runner.require([sys.executable, '-c',
            'import sys; sys.path.insert(0, sys.argv[1]); from llvm_import import LLVM; '
            'print(".".join(map(str, LLVM(sys.argv[2]).version)))', str(IMPORTER.parent), args.libllvm])
        report['environment']['llvm_version'] = Path(probe['stdout']).read_text().strip()
        target_probe = runner.require([args.clangxx, '-dumpmachine'])
        target = args.target or Path(target_probe['stdout']).read_text().strip()
        report['environment']['target'] = target
        target_flags = ['-target', target]
        # Preflight failure is unavailable/incompatible infrastructure, not a feature rejection.
        if not re.match(r'(arm64|aarch64)-apple-', target):
            raise Failed('current importer requires Apple AArch64; target is ' + target)
        driver = out / 'driver.o'
        runner.require([args.clang, *target_flags, '-O2', '-c', out/'sources/driver.c', '-o', driver])
        boundaries = []
        for name, compiler in [('clang', args.clang), ('alternate', args.abi_cc)]:
            if compiler:
                obj = out / f'boundary-{name}.o'
                # Alternate compiler uses its own native target; link/run establishes compatibility.
                flags = target_flags if name == 'clang' else []
                runner.require([compiler, *flags, '-O2', '-c', out/'sources/boundary.c', '-o', obj])
                boundaries.append((name, obj))
        if args.abi_rustc:
            obj = out/'boundary-rustc.o'
            runner.require([args.abi_rustc, '--edition', '2024', '--crate-type', 'lib',
                            '-C', 'opt-level=2', '-C', 'panic=abort', '--emit', 'obj',
                            out/'sources/boundary.rs', '-o', obj])
            boundaries.append(('rustc', obj))
        report['mixed_compiler'] = {'requested': bool(args.abi_cc or args.abi_rustc),
            'rustc_boundary': bool(args.abi_rustc),
            'note': 'alternate C compiler recorded by version; same-family aliases are not evidence of distinct compilers'
                    if args.abi_cc else ('independent Rust C-ABI boundary selected' if args.abi_rustc else
                    'not exercised: pass --abi-cc or --abi-rustc to select an independent compiler')}
        report['status'] = 'running_cases'
        for name, feature, required, level in CASES:
            if args.cases and name not in args.cases:
                continue
            directory = out / name
            directory.mkdir()
            case = {'name': name, 'feature': feature, 'required': required,
                    'ir_optimization': level, 'status': 'running', 'stage': 'clang_ir', 'native': [], 'differential': []}
            report['cases'].append(case)
            flags = [*target_flags, '-std=c++17', level]
            if name != 'eh':
                flags += ['-fno-exceptions', '-fno-rtti']
            case['flags'] = flags
            ir, dest = directory/'input.ll', directory/'imported.keel'
            try:
                runner.require([args.clangxx, *flags, '-S', '-emit-llvm', out/f'sources/{name}.cpp', '-o', ir])
                original_hash = digest(ir)
                case['ir_sha256'] = original_hash
                ir_text = ir.read_text()
                case['ir_target'] = re.search(r'^target triple = "(.*)"', ir_text, re.M).group(1)
                case['data_layout'] = re.search(r'^target datalayout = "(.*)"', ir_text, re.M).group(1)
                case['ir_indicators'] = {token: len(re.findall(pattern, ir_text, re.M))
                    for token, pattern in {
                        'nsw': r'\bnsw\b', 'nuw': r'\bnuw\b',
                        'alloca': r'\balloca\b', 'getelementptr': r'\bgetelementptr\b',
                        'phi': r'\bphi\b', 'invoke': r'\binvoke\b',
                        'global_definitions': r'^@[^\n]*=',
                        'conditional_branches': r'^\s*br i1 ',
                    }.items()}
                case['stage'] = 'native_reference'
                # Reference object is compiled from exactly the IR submitted to the importer.
                reference_obj = directory/'reference.o'
                runner.require([args.clangxx, *target_flags, '-c', ir, '-o', reference_obj])
                expected = {}
                for boundary_name, boundary in boundaries:
                    exe = directory / f'reference-{boundary_name}'
                    runner.require([args.clangxx, *target_flags, driver, boundary, reference_obj, '-o', exe])
                    result = runner.require([exe])
                    output = Path(result['stdout']).read_bytes()
                    if len(output.splitlines()) != 4241 or not output.splitlines()[-1].startswith(b'cases=4240 checksum='):
                        raise Failed('invalid native reference output')
                    expected[boundary_name] = output
                    if output != expected['clang']:
                        raise Failed('alternate C boundary differs from Clang reference')
                    case['native'].append({'boundary': boundary_name, 'status': 'passed', 'output_sha256': hashlib.sha256(output).hexdigest()})
                case['stage'] = 'import'
                dest.write_bytes(SENTINEL)
                result = runner.run([sys.executable, IMPORTER, '--libllvm', args.libllvm,
                                     '--keel', args.keel, ir, '-o', dest])
                stdout = Path(result['stdout']).read_bytes()
                stderr = Path(result['stderr']).read_text(errors='replace')
                preserved = dest.exists() and dest.read_bytes() == SENTINEL
                status = classify_import(result['returncode'], stdout, stderr, preserved)
                case['import'] = {'status': status, 'returncode': result['returncode'],
                                  'diagnostic': stderr, 'destination_preserved': preserved,
                                  'stdout': result['stdout'], 'stderr': result['stderr']}
                if digest(ir) != original_hash:
                    raise Failed('input IR changed during import')
                if status == 'rejected':
                    case['status'] = 'required_rejected' if required else 'rejected'
                    continue
                if status != 'accepted':
                    raise Failed('import failed: ' + status)
                for tier in report['tiers']:
                    case['stage'] = 'keel_compile_' + tier
                    obj = directory / f'{tier}.o'
                    runner.require([args.keel, 'compile', '--opt-tier', tier, '--emit-object', dest, '-o', obj])
                    for boundary_name, boundary in boundaries:
                        exe = directory / f'{tier}-{boundary_name}'
                        runner.require([args.clangxx, *target_flags, driver, boundary, obj, '-o', exe])
                        case['stage'] = 'native_differential_' + tier + '_' + boundary_name
                        actual = runner.require([exe])
                        matches = Path(actual['stdout']).read_bytes() == expected[boundary_name]
                        case['differential'].append({'tier': tier, 'boundary': boundary_name,
                                                     'status': 'passed' if matches else 'mismatch',
                                                     'stdout': actual['stdout']})
                        if not matches:
                            raise Failed(f'native differential mismatch at {tier}/{boundary_name}')
                case.update(status='passed', stage='complete')
            except (Failed, OSError, ValueError, AttributeError) as e:
                case.update(status='failed', error=str(e))
        counts = Counter(c['status'] for c in report['cases'])
        report['summary'] = dict(counts)
        exit_code = 0 if all(c['status'] in ('passed', 'rejected') for c in report['cases']) else 1
        report['status'] = 'passed_with_blockers' if exit_code == 0 and counts['rejected'] else ('passed' if exit_code == 0 else 'failed')
    except (Failed, OSError, ValueError) as e:
        report.update(status='prerequisite_failure', error=str(e))
        exit_code = 2
    finally:
        report['exit_code'] = exit_code
        (out/'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'status': report['status'], 'summary': report.get('summary', {}),
                          'report': str(out/'report.json'), 'exit_code': exit_code}))
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())
