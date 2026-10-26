#!/usr/bin/env python3
"""Capture and probe unchanged Kbuild saved inputs for a 64-bit kernel audit.

Build with KCFLAGS=-save-temps=obj KAFLAGS=-save-temps=obj first. This does not
wrap or impersonate GCC, modify Linux inputs, or qualify generated code.
"""
import argparse
import collections
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def external_reason(relative, command, text):
    if relative.startswith(('scripts/', 'tools/')):
        return 'build helper remains external, even when compiled with target flags'
    if relative.startswith(('arch/x86/boot/', 'arch/x86/realmode/rm/')):
        return 'boot/real-mode code remains external'
    if relative.startswith(('arch/x86/entry/vdso/vdso32/', 'arch/x86/ia32/')):
        return 'compatibility-mode code remains external'
    if re.search(r'(?:^|\s)-m(?:16|32)(?:\s|$)', command):
        return '16/32-bit compiler invocation remains external'
    if re.search(r'^\s*\.code(?:16|32)', text, re.M):
        return 'mixed/legacy-mode assembly remains external'
    return None


def capture(args):
    build = args.build.resolve()
    source = args.source.resolve()
    rows = []
    for path in sorted(build.rglob('*')):
        if path.suffix not in ('.i', '.s'):
            continue
        relative = path.relative_to(build).as_posix()
        # Host programs and generated source fragments are outside this corpus.
        cmd_path = path.with_name('.' + path.stem + '.o.cmd')
        if not cmd_path.exists():
            continue
        saved = re.search(r'^savedcmd_.*? := (.*)$', cmd_path.read_text(), re.M)
        if not saved:
            continue
        command = saved[1]
        if not re.search(r'(?:^|\s)-D__KERNEL__(?:\s|$)', command):
            continue
        text = path.read_text(errors='replace')
        reason = external_reason(relative, command, text)
        rows.append(dict(path=relative, kind='c' if path.suffix == '.i' else 'asm',
                         sha256=sha(path), bytes=path.stat().st_size,
                         scope='external' if reason else 'kernel64', reason=reason,
                         command=command.replace(str(source), '<source>')
                                        .replace(str(build), '<build>')))
    if not rows:
        raise SystemExit('No Kbuild saved inputs found; build with -save-temps=obj.')
    write(args.out, dict(schema=1, config_sha256=sha(build / '.config'),
                         inputs=rows, counts=dict(collections.Counter(
                             x['scope'] + '/' + x['kind'] for x in rows))))


def category(message, source_line=''):
    message = re.sub(r'^keel: .*?: line \d+: ', '', message)
    if 'asm goto' in message:
        return 'asm-goto'
    if 'inline assembly' in message:
        return 'x86-inline-asm'
    if 'expected a type, found `asm`' in message:
        return 'file-scope-asm'
    if ('unsupported AT&T instruction ``' in message and
            re.match(r'\s*(?:rep|repe|repz|repne|repnz|lock)\s*(?:;|$)', source_line)):
        return 'standalone-prefix'
    if 'unsupported ELF directive' in message or 'unsupported directive' in message:
        return 'assembler-directive:' + next(iter(re.findall(r'`([^`]+)`', message)), '?')
    if re.search(r'unsupported (?:x86|AT&T) instruction', message):
        return 'instruction-binding:' + next(iter(re.findall(r'`([^`]+)`', message)), '?')
    if 'requires x86 target features' in message:
        return 'feature-policy'
    # Collapse changing paths, line/column numbers and identifier names.
    message = re.sub(r'^.*?:\d+(?::\d+)?:\s*(?:error:\s*)?', '', message)
    return re.sub(r'`[^`]+`', '`*`', message)[:180]


def probe(args):
    build = args.build.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    manifest = json.loads(args.manifest.read_text())
    if sha(build / '.config') != manifest['config_sha256']:
        raise SystemExit('Resolved config differs from captured manifest.')
    keel_sha256 = sha(args.keel)
    rows = []
    for item in manifest['inputs']:
        if item['scope'] != 'kernel64':
            continue
        path = build / item['path']
        if sha(path) != item['sha256']:
            raise SystemExit('Changed input: ' + item['path'])
        index = len(rows)
        command = [str(args.keel.resolve()), 'cc' if item['kind'] == 'c' else 'asm',
                   '--target', 'x86_64-unknown-linux-gnu']
        if item['kind'] == 'asm':
            command += ['--x86-syntax', 'att', '--cpu-level', 'v3']
        else:
            command += ['--emit-object']
        command += [str(path), '-o', str(args.out.resolve() / f'{index}.o')]
        try:
            run = subprocess.run(command, cwd=build, capture_output=True, timeout=args.timeout)
            diagnostic = run.stderr.decode(errors='replace').strip()
            status = ('accepted' if run.returncode == 0 else
                      'crashed' if run.returncode < 0 or run.returncode == 101 else 'rejected')
            code = run.returncode
        except subprocess.TimeoutExpired:
            diagnostic, status, code = 'audit timeout', 'timeout', None
        diagnostic = diagnostic.replace(str(build), '<build>')
        message = diagnostic.splitlines()[0] if diagnostic else ''
        location = re.search(r': line (\d+):', message)
        lines = path.read_text(errors='replace').splitlines() if location else []
        line_index = int(location[1]) - 1 if location else -1
        source_line = lines[line_index] if 0 <= line_index < len(lines) else ''
        rows.append(dict(path=item['path'], kind=item['kind'], sha256=item['sha256'],
                         status=status, returncode=code, diagnostic=diagnostic,
                         first_source_line=source_line,
                         category=category(message, source_line) if status == 'rejected' else status))
        # Preserve progress if the audit is interrupted.
        write(args.out / 'results.json', dict(schema=1,
              note='First diagnostics on unchanged reference inputs; not exhaustive gaps or execution evidence.',
              config_sha256=manifest['config_sha256'],
              keel_sha256=keel_sha256, results=rows,
              categories=dict(collections.Counter(x['category'] for x in rows))))
    print(json.dumps(collections.Counter(x['category'] for x in rows), indent=2))


def compare(args):
    """Reuse the suite's ELF normalization; do not infer equality from acceptance."""
    inspector_path = Path(__file__).resolve().parent.parent / 'compiler_asm_compat' / 'objinspect.py'
    spec = importlib.util.spec_from_file_location('kernel_objinspect', inspector_path)
    inspector = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(inspector)
    build = args.build.resolve()
    probe_out = args.probe_out.resolve()
    results = json.loads((probe_out / 'results.json').read_text())
    if sha(build / '.config') != results['config_sha256']:
        raise SystemExit('Resolved config differs from probe results.')
    args.out.mkdir(parents=True, exist_ok=False)
    rows = []
    for index, item in enumerate(results['results']):
        if item['kind'] != 'asm' or item['status'] != 'accepted':
            continue
        source = build / item['path']
        if sha(source) != item['sha256']:
            raise SystemExit('Changed input: ' + item['path'])
        actual = probe_out / f'{index}.o'
        reference = args.out.resolve() / f'{index}.gnu.o'
        run = subprocess.run([args.gnu_as, '--64', '-mrelax-relocations=no',
                              str(source), '-o', str(reference)], cwd=build, capture_output=True)
        row = dict(path=item['path'], input_sha256=item['sha256'],
                   keel_object_sha256=sha(actual), reference_returncode=run.returncode)
        if run.returncode:
            row['reference_diagnostic'] = run.stderr.decode(errors='replace')
        else:
            row['reference_object_sha256'] = sha(reference)
            expected, got = inspector.parse(reference), inspector.parse(actual)
            row['comparison'] = inspector.compare(expected, got)
            shared = [r for r in row['comparison']['sections'].values() if r.get('present')]
            row['shared_section_bytes_equal'] = all(r['bytes'] for r in shared)
            row['shared_section_relocations_equal'] = all(r['relocations'] for r in shared)
            row['shared_section_layout_equal'] = all(all(r[k] for k in (
                'size', 'type_flags', 'align_equal', 'entsize')) for r in shared)
            row['public_symbols_equal'] = ([s for s in expected['symbols'] if s[1] != 0] ==
                                           [s for s in got['symbols'] if s[1] != 0])
        rows.append(row)
    report = dict(schema=1, config_sha256=results['config_sha256'],
                  keel_sha256=results['keel_sha256'],
                  gnu_as_version=subprocess.check_output([args.gnu_as, '--version'], text=True).splitlines()[0],
                  reference_flags=['--64', '-mrelax-relocations=no'],
                  note='Unmodified accepted assembly inputs; GNU relaxation disabled to match the existing relocation contract. No objtool, link or execution of Keel objects.',
                  total=len(rows),
                  reference_failed=sum(r['reference_returncode'] != 0 for r in rows),
                  all_equal=sum(r.get('comparison', {}).get('all_equal', False) for r in rows),
                  code_data_equal=sum(r.get('comparison', {}).get('code_data_equal', False) for r in rows),
                  shared_section_bytes_equal=sum(r.get('shared_section_bytes_equal', False) for r in rows),
                  shared_section_relocations_equal=sum(r.get('shared_section_relocations_equal', False) for r in rows),
                  shared_section_layout_equal=sum(r.get('shared_section_layout_equal', False) for r in rows),
                  public_symbols_equal=sum(r.get('public_symbols_equal', False) for r in rows),
                  results=rows)
    write(args.out / 'comparison.json', report)
    print(json.dumps({k:report[k] for k in ('total', 'reference_failed', 'all_equal', 'code_data_equal')}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    cap = sub.add_parser('capture')
    cap.add_argument('--source', type=Path, required=True)
    cap.add_argument('--build', type=Path, required=True)
    cap.add_argument('--out', type=Path, required=True)
    check = sub.add_parser('probe')
    check.add_argument('--build', type=Path, required=True)
    check.add_argument('--manifest', type=Path, required=True)
    check.add_argument('--keel', type=Path, required=True)
    check.add_argument('--out', type=Path, required=True)
    check.add_argument('--timeout', type=int, default=60)
    oracle = sub.add_parser('compare')
    oracle.add_argument('--build', type=Path, required=True)
    oracle.add_argument('--probe-out', type=Path, required=True)
    oracle.add_argument('--out', type=Path, required=True)
    oracle.add_argument('--gnu-as', default='as')
    args = parser.parse_args()
    dict(capture=capture, probe=probe, compare=compare)[args.action](args)


if __name__ == '__main__':
    main()
