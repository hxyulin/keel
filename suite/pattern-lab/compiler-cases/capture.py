#!/usr/bin/env python3
"""Explicit, bounded compiler-only capture. Never builds, links or runs kernels."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import stat
import subprocess
import time

FAMILIES = ('madd', 'rotate', 'udiv10', 'branch-select', 'sext8', 'narrow-load')
TARGETS = ('aarch64-apple-darwin', 'x86_64-unknown-linux-gnu')
HERE = Path(__file__).resolve().parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def regular(path, maximum):
    path = path.resolve(strict=True)
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_size > maximum:
        raise ValueError('expected bounded regular file: ' + str(path))
    return path


def instructions(assembly, symbol):
    """One externally visible function, never a following function or directive.

    Labels are kept out of the instruction list. Unknown/missing boundaries fail
    closed. This parses Keel GAS syntax, including Mach-O underscore prefixes.
    """
    lines = assembly.splitlines()
    label = r'(?:_?' + re.escape(symbol) + r'|"_?' + re.escape(symbol) + r'")'
    starts = [i for i, line in enumerate(lines)
              if re.fullmatch(r'\s*' + label + r':\s*(?://.*)?', line)]
    if len(starts) != 1:
        raise ValueError('missing/duplicate function label: ' + symbol)
    body = []
    for line in lines[starts[0] + 1:]:
        line = line.split('//', 1)[0].split('# ', 1)[0].strip()
        if re.match(r'\.(?:cfi_endproc|size|globl|global)\b', line):
            break
        if not line or line.startswith('.') or line.endswith(':'):
            continue
        # An unknown nonlocal label cannot silently merge two functions.
        if ':' in line or not re.match(r'^[a-z][a-z0-9.]*\s*', line):
            raise ValueError('unknown assembly line: ' + line)
        body.append(line.lower())
    else:
        raise ValueError('unbounded function: ' + symbol)
    if not body:
        raise ValueError('empty function: ' + symbol)
    return body


def shape(assembly, family, target):
    """Recognize instruction features; not a dataflow or equivalence proof.

    `supported` means candidate-like feature observed, `missed` means a
    baseline-like feature only. Alternative is valid emitted code whose shape
    this deliberately small recognizer cannot classify. Loop branches alone
    never establish the branch-select baseline.
    """
    try:
        body = instructions(assembly, 'bridge_' + family.replace('-', '_'))
    except ValueError as exc:
        return {'status': 'unsupported', 'reason': str(exc), 'evidence': []}
    arm = target == TARGETS[0]
    candidates = {
        'madd': r'^madd\b' if arm else r'^lea[q]?\b.*(?:\*\s*2|,\s*2\))',
        'rotate': r'^ror\b.*#13\b' if arm else r'^ror[q]?\b.*(?:13|0xd)\b',
        'udiv10': r'^umulh\b' if arm else r'^mul[q]?\b',
        'branch-select': r'^csel\b' if arm else r'^cmov[a-z]*\b',
        'sext8': r'^sxtb\b' if arm else r'^movs(?:bq|x)\b.*(?:byte|\b[a-z]+l\b)',
        'narrow-load': r'^ldrb\b' if arm else r'^movz(?:bq|bl|x)\b.*(?:byte|\()',
    }
    baselines = {
        'madd': r'^mul\b' if arm else r'^imul[q]?\b',
        'rotate': r'^orr\b' if arm else r'^or[q]?\b',
        'udiv10': r'^udiv\b' if arm else r'^div[q]?\b',
        'branch-select': r'^(?:tbz|tbnz)\b.*#0\b' if arm else r'(?!)',
        'sext8': r'^asr\b.*#56\b' if arm else r'^sar[q]?\b.*56\b',
        'narrow-load': r'^and\b.*#(?:255|0xff)\b' if arm else r'^and[q]?\b.*(?:255|0xff)\b',
    }
    candidate = [s for s in body if re.search(candidates[family], s)]
    baseline = [s for s in body if re.search(baselines[family], s)]
    return {'status': 'supported' if candidate else 'missed' if baseline else 'alternative',
            'evidence': candidate or baseline, 'instructions': body,
            'instruction_count': len(body),
            'code_size_bytes': None,
            'code_size_reason': 'No assembler/object execution; instruction count is not object size',
            'recognition_scope': 'function instruction features only; no operand dataflow proof'}


def git_state():
    result = {}
    for key, args in [('commit', ['rev-parse', 'HEAD']),
                      ('tree', ['rev-parse', 'HEAD^{tree}']),
                      ('status', ['status', '--porcelain'])]:
        p = subprocess.run(['git', '-C', str(HERE), *args], capture_output=True,
                           timeout=5, stdin=subprocess.DEVNULL)
        result[key] = {'argv': p.args, 'returncode': p.returncode,
                       'stdout': p.stdout.decode(errors='replace'),
                       'stderr': p.stderr.decode(errors='replace')}
    return result


def capture(args):
    # Validate before any compiler invocation. Build provenance is externally
    # supplied, never inferred from a nearby git checkout or executable name.
    compiler = regular(args.compiler, 256 * 1024 * 1024)
    identity_path = regular(args.identity, 1024 * 1024)
    identity = json.loads(identity_path.read_text())
    for key in ('sha256', 'revision', 'tree', 'build_command', 'toolchain'):
        if key not in identity or not identity[key]:
            raise ValueError('identity requires nonempty ' + key)
    if identity['sha256'] != digest(compiler):
        raise ValueError('compiler SHA-256 differs from supplied identity')
    source = regular(HERE / 'recurrences.c', 65536)
    args.output.mkdir(parents=False, exist_ok=False)
    output = args.output.resolve()
    report = {'schema': 1, 'status': 'capturing', 'identity': identity,
              'identity_sha256': digest(identity_path), 'compiler': str(compiler),
              'provenance_caveat': 'Build provenance supplied by caller; binary hash verified',
              'host': platform.uname()._asdict(), 'source': str(source),
              'source_sha256': digest(source), 'capture_script_sha256': digest(Path(__file__)),
              'git': git_state(),
              'protocol': {'timeout_seconds': args.timeout, 'total_seconds': 180,
                           'order': 'target, raw/optimized, IR/assembly',
                           'native_execution': False}, 'commands': [], 'shapes': []}
    # Header/macro-free C is also valid preprocessed input. Identical bytes bypass
    # SDK discovery and host headers on both targets; do not run a preprocessor.
    input_path = output / 'recurrences.i'
    input_path.write_bytes(source.read_bytes())
    (output / 'compiler-identity.json').write_bytes(identity_path.read_bytes())
    started = time.monotonic()
    try:
        for target in TARGETS:
            for optimized in (False, True):
                for kind in ('ir', 'asm'):
                    tag = target + ('-opt' if optimized else '-raw') + '.' + kind
                    artifact = output / tag
                    argv = [str(compiler), 'cc', '--target', target]
                    if optimized:
                        argv += ['-O']
                    if kind == 'ir':
                        argv += ['--emit-ir']
                    argv += [str(input_path), '-o', str(artifact)]
                    record = {'argv': argv, 'target': target, 'optimized': optimized,
                              'kind': kind, 'artifact': str(artifact)}
                    report['commands'].append(record)
                    remaining = 180 - (time.monotonic() - started)
                    try:
                        if remaining <= 0:
                            raise subprocess.TimeoutExpired(argv, 0)
                        proc = subprocess.run(argv, stdin=subprocess.DEVNULL,
                                              capture_output=True,
                                              timeout=min(args.timeout, remaining))
                        stdout, stderr = proc.stdout, proc.stderr
                        record['returncode'] = proc.returncode
                    except subprocess.TimeoutExpired as exc:
                        stdout, stderr = exc.stdout or b'', exc.stderr or b''
                        record['failure'] = ('total deadline exhausted; command not started' if remaining <= 0
                                             else 'timeout; direct compiler child reaped by subprocess.run')
                    except OSError as exc:
                        stdout, stderr = b'', b''
                        record['failure'] = str(exc)
                    for stream, content in [('stdout', stdout), ('stderr', stderr)]:
                        path = output / (tag + '.' + stream)
                        path.write_bytes(content)
                        record[stream] = {'path': str(path), 'sha256': digest(path)}
                    valid = record.get('returncode') == 0 and artifact.is_file() and artifact.stat().st_size > 0
                    record['status'] = 'emitted' if valid else 'unsupported'
                    if artifact.is_file():
                        record['sha256'] = digest(artifact)
                    if kind == 'asm':
                        assembly = artifact.read_text() if valid else ''
                        for family in FAMILIES:
                            observed = shape(assembly, family, target) if valid else {
                                'status': 'unsupported', 'evidence': [], 'reason': 'compiler emission failed'}
                            report['shapes'].append(dict(family=family, target=target,
                                optimized=optimized, measured_id=('arm' if target == TARGETS[0] else 'x86') + '-more-' + family,
                                artifact=str(artifact), **observed))
        report['status'] = 'captured' if all(c.get('status') == 'emitted' for c in report['commands']) else 'partial_failure'
    finally:
        report['compiler_sha256_after'] = digest(compiler)
        report['source_sha256_after'] = digest(source)
        if report['compiler_sha256_after'] != identity['sha256'] or report['source_sha256_after'] != report['source_sha256']:
            report['status'] = 'identity_changed'
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if report['status'] == 'captured' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execute', action='store_true', help='explicit compiler execution opt-in; requires root lease')
    parser.add_argument('--compiler', type=Path, required=True)
    parser.add_argument('--identity', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='new directory under an existing parent')
    parser.add_argument('--timeout', type=float, default=20)
    args = parser.parse_args()
    if not args.execute:
        parser.error('--execute required; obtain root build/native lease first')
    if not 0.05 <= args.timeout <= 30:
        parser.error('--timeout must be 0.05..30 seconds')
    try:
        return capture(args)
    except (ValueError, OSError, subprocess.SubprocessError) as exc:
        parser.exit(2, str(exc) + '\n')


if __name__ == '__main__':
    raise SystemExit(main())
