#!/usr/bin/env python3
"""Capture, probe and replay Kbuild inputs for the 64-bit kernel audit.

Build with KCFLAGS=-save-temps=obj KAFLAGS=-save-temps=obj before capture.
"""
import argparse
import collections
import concurrent.futures
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shlex
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
    if 'stack-pointer asm outputs require a stack-call contract' in message:
        return 'asm-stack-call-contract'
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


def feature_gate(message):
    """Record the instruction's requirements, not inferred missing permissions."""
    match = re.search(
        r'(?:^|:\s+)([a-z][a-z0-9_]*(?: [a-z][a-z0-9_]*)*)'
        r' requires x86 target features: ([a-z0-9_]+(?:,[a-z0-9_]+)*)$',
        message,
    )
    if not match:
        return None
    return dict(instruction=match[1], required_features=match[2].split(','))


def resolve_feature_policy(args):
    """Resolve audit-only permissions without changing Keel CPU profiles."""
    features = {'c': args.c_target_features, 'asm': args.asm_target_features}
    path = getattr(args, 'feature_policy', None)
    if path is None:
        return features, None
    if any(value is not None for value in features.values()):
        raise SystemExit('--feature-policy cannot be combined with target-feature overrides.')
    try:
        raw = path.read_bytes()
        policy = json.loads(raw)
    except (OSError, ValueError) as error:
        raise SystemExit(f'Cannot load feature policy: {error}') from error
    if (not isinstance(policy, dict) or policy.get('schema') != 1
            or policy.get('target') != 'x86_64-unknown-linux-gnu'):
        raise SystemExit('Feature policy requires schema 1 and the x86-64 Linux target.')
    name = policy.get('name')
    if not isinstance(name, str) or not name.strip():
        raise SystemExit('Feature policy requires a nonempty name.')
    fields = Path(__file__).resolve().parents[3] / 'tools/x86-isa/feature-fields.json'
    known = {field['field'] for field in json.loads(fields.read_text())}
    for kind in features:
        values = policy.get(kind + '_target_features')
        if (not isinstance(values, list)
                or any(not isinstance(value, str) or value not in known for value in values)
                or len(values) != len(set(values))):
            raise SystemExit(f'Feature policy {kind} permissions must be unique canonical feature names.')
        features[kind] = ','.join('+' + value for value in values) or None
    return features, {'name': name, 'sha256': hashlib.sha256(raw).hexdigest()}


def raw_c_command(saved, keel, output, features):
    """Replace the compiler and redirect its artifacts. Forward every flag."""
    head, _, _ = saved.partition(' ; ')
    original = shlex.split(head)
    if not original or original.count('-c') != 1 or original.count('-o') != 1:
        raise ValueError('Expected one saved Kbuild C compilation with an object output.')
    if any(arg in (';', '&&', '||', '|', '<', '>') for arg in original):
        raise ValueError('Unexpected shell operator inside the compiler command.')
    switches = ['--target', 'x86_64-unknown-linux-gnu', '--link-intent', 'executable']
    if features is not None:
        switches += ['--target-features', features]
    args = original[1:]
    args[args.index('-o') + 1] = str(output)
    dependencies = []
    for index, arg in enumerate(args):
        if arg.startswith('-Wp,-MMD,'):
            name = Path(arg.removeprefix('-Wp,-MMD,')).name
            dep = output.parent / name
            args[index] = f'-Wp,-MMD,{dep}'
            dependencies.append(dep)
    return [str(keel), 'cc', *switches, *args], dependencies


def replay_c(args):
    """Replay one reference build's commands on the manifest's C file list."""
    args.c_target_features = args.asm_target_features = None
    features, policy = resolve_feature_policy(args)
    build, out, keel = args.build.resolve(), args.out.resolve(), args.keel.resolve()
    if args.jobs < 1 or args.timeout < 1:
        raise SystemExit('Jobs and timeout must be positive.')
    manifest = json.loads(args.manifest.read_text())
    inputs = [row for row in manifest['inputs'] if (row['scope'], row['kind']) == ('kernel64', 'c')]
    if not inputs:
        raise SystemExit('The manifest has no 64-bit C inputs.')
    out.mkdir(parents=True, exist_ok=False)

    def compile_one(row):
        path = Path(row['path'])
        output = out / path.with_suffix('.o')
        output.parent.mkdir(parents=True, exist_ok=True)
        cmd_path = build / path.with_name('.' + path.stem + '.o.cmd')
        try:
            saved = re.search(r'^savedcmd_.*? := (.*)$', cmd_path.read_text(), re.M)
            if not saved or not re.search(r'(?:^|\s)-D__KERNEL__(?:\s|$)', saved[1]):
                raise ValueError(f'{cmd_path}: missing saved kernel compiler command.')
            reason = external_reason(row['path'], saved[1], '')
            if reason:
                raise ValueError(f'{cmd_path}: {reason}.')
            argv, dependencies = raw_c_command(saved[1], keel, output, features['c'])
            result = subprocess.run(argv, cwd=build, capture_output=True, text=True,
                                    errors='replace', timeout=args.timeout)
            record = dict(path=row['path'], argv=argv, returncode=result.returncode,
                          diagnostic=result.stderr)
            if result.returncode == 0:
                artifacts = [output, *dependencies]
                if '-save-temps=obj' in argv:
                    artifacts += [output.with_suffix('.i'), output.with_suffix('.s')]
                for artifact in artifacts:
                    if not artifact.is_file():
                        raise ValueError(f'Missing emitted artifact: {artifact}')
                for dep in dependencies:
                    if not dep.read_text().startswith(f'{output}:'):
                        raise ValueError(f'Invalid dependency target: {dep}')
                record['object_sha256'] = sha(output)
            return record
        except (OSError, ValueError, subprocess.TimeoutExpired) as error:
            return dict(path=row['path'], returncode=None, diagnostic=str(error))

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as executor:
        results = list(executor.map(compile_one, inputs))
    accepted = sum(row['returncode'] == 0 for row in results)
    write(out / 'results.json', dict(schema=1, config_sha256=sha(build / '.config'),
                                     manifest_sha256=sha(args.manifest), keel_sha256=sha(keel),
                                     feature_policy=policy, accepted=accepted, inputs=results))
    print(f'{accepted}/{len(results)} raw C commands compiled.')
    if accepted != len(results):
        raise SystemExit(1)


def probe(args):
    features, policy = resolve_feature_policy(args)
    build = args.build.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    manifest = json.loads(args.manifest.read_text())
    if sha(build / '.config') != manifest['config_sha256']:
        raise SystemExit('Resolved config differs from captured manifest.')
    keel_sha256 = sha(args.keel)
    rows = []
    feature_gates = collections.Counter()
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
            command += ['--emit-object', '--cpu-level', 'v1']
        extra_features = features[item['kind']]
        if extra_features is not None:
            command += ['--target-features', extra_features]
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
        row = dict(path=item['path'], kind=item['kind'], sha256=item['sha256'],
                   status=status, returncode=code, diagnostic=diagnostic,
                   first_source_line=source_line,
                   category=category(message, source_line) if status == 'rejected' else status)
        gate = feature_gate(message) if status == 'rejected' else None
        if gate:
            row['feature_gate'] = gate
            feature_gates[(item['kind'], gate['instruction'], tuple(gate['required_features']))] += 1
        rows.append(row)
        # Preserve progress if the audit is interrupted.
        write(args.out / 'results.json', dict(schema=1,
              note='First diagnostics on unchanged reference inputs; not exhaustive gaps or execution evidence.',
              config_sha256=manifest['config_sha256'],
              keel_sha256=keel_sha256, results=rows,
              target_policy={'c': {'cpu_level': 'v1', 'target_features': features['c']},
                             'asm': {'cpu_level': 'v3', 'target_features': features['asm']}},
              **({'feature_policy': policy} if policy is not None else {}),
              first_error_feature_gates=[
                  dict(kind=kind, instruction=instruction, required_features=list(features),
                       rejected_inputs=count)
                  for (kind, instruction, features), count in sorted(feature_gates.items())],
              categories=dict(collections.Counter(x['category'] for x in rows))))
    print(json.dumps(collections.Counter(x['category'] for x in rows), indent=2))


def compare(args):
    """Reuse the suite's ELF normalization; do not infer equality from acceptance."""
    inspector_path = Path(__file__).resolve().parents[3] / 'suite/asm/compiler_asm_compat/objinspect.py'
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
    check.add_argument('--feature-policy', type=Path, help='Audit feature-policy JSON; cannot be combined with target-feature overrides.')
    check.add_argument('--asm-target-features', help='Explicit extra assembler features; omit for the pinned v3 policy.')
    check.add_argument('--c-target-features', help='Explicit extra C target features; omit for the pinned v1 policy.')
    replay = sub.add_parser('replay-c', help='Replay saved GCC or Clang C compiler commands.')
    for name in ('build', 'manifest', 'keel', 'out'):
        replay.add_argument('--' + name, type=Path, required=True)
    replay.add_argument('--jobs', type=int, default=4)
    replay.add_argument('--timeout', type=int, default=180)
    replay.add_argument('--feature-policy', type=Path,
                        default=Path(__file__).with_name('kernel-audit-policy.json'))
    oracle = sub.add_parser('compare')
    oracle.add_argument('--build', type=Path, required=True)
    oracle.add_argument('--probe-out', type=Path, required=True)
    oracle.add_argument('--out', type=Path, required=True)
    oracle.add_argument('--gnu-as', default='as')
    args = parser.parse_args()
    {'capture': capture, 'probe': probe, 'compare': compare, 'replay-c': replay_c}[args.action](args)


if __name__ == '__main__':
    main()
