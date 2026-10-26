#!/usr/bin/env python3
"""Capture and probe unchanged Kbuild saved inputs for a 64-bit kernel audit.

Build with KCFLAGS=-save-temps=obj KAFLAGS=-save-temps=obj first. This does not
wrap or impersonate GCC, modify Linux inputs, or qualify generated code.
"""
import argparse
import collections
import hashlib
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
    args = parser.parse_args()
    (capture if args.action == 'capture' else probe)(args)


if __name__ == '__main__':
    main()
