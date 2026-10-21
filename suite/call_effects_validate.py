#!/usr/bin/env python3
"""Correctness/static evidence only. Run once under fast_opt_locked.py."""
import argparse
import hashlib
import html
import json
import pathlib
import re
import subprocess
from call_load_reuse_validate import functions, loads, text_section


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--keel', type=pathlib.Path, required=True)
    ap.add_argument('--out', type=pathlib.Path, required=True)
    ap.add_argument('--originals', type=pathlib.Path, nargs='*', default=[])
    args = ap.parse_args()
    root = pathlib.Path(__file__).resolve().parents[1]
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    keel = str(args.keel.resolve())
    fixture = root/'crates/keel/tests/fixtures/call_effects.c'
    driver = root/'crates/keel/tests/fixtures/call_effects_driver.c'
    digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    report = dict(base='dafc74db4dedac2351eea2c04e07c7adce9a9504', commands=[], targets=[], original_c=[],
                  compiler=dict(path=keel, sha256=digest(pathlib.Path(keel))), source_sha256={},
                  limitations=['No timing or runtime performance claim.', 'x86 object routes checked but not executed.',
                               'Only single-block scalar/no-memory direct bodies; no readonly/capture inference.',
                               'Maximum eight contained call edges; original CFG budgets unchanged.',
                               'Summary ablation inserts a memory-clobber asm in private bodies containing direct calls; used only for static attribution.'])

    def save():
        (out/'report.json').write_text(json.dumps(report, indent=2)+'\n')
        (out/'report.html').write_text('<!doctype html><meta charset="utf-8"><title>Bounded call effects evidence</title><style>body{font:15px system-ui;max-width:1100px;margin:32px auto}pre{white-space:pre-wrap;overflow-wrap:anywhere}</style><h1>Bounded transitive call effects</h1><p>Correctness and static eliminations only. Default pipelines unchanged.</p><pre>'+html.escape(json.dumps(report, indent=2))+'</pre>')

    def run(*argv):
        argv = list(map(str, argv))
        result = subprocess.run(argv, capture_output=True, text=True)
        report['commands'].append(dict(argv=argv, returncode=result.returncode, stdout=result.stdout, stderr=result.stderr))
        save()
        if result.returncode:
            raise RuntimeError(f'{argv}: {result.stdout}\n{result.stderr}')
        return result.stdout

    def ablate(source):
        def replace(match):
            body = match[0]
            if re.search(r'\b(local|noninterposable)\b', body.splitlines()[0]) and ' = call @' in body:
                body = re.sub(r'^(block[^\n]*:\n)', r'\1    asm "", "~{memory}"()\n', body, count=1, flags=re.M)
            return body
        return re.sub(r'^func @.*?^}', replace, source, flags=re.M | re.S)

    for p in [root/'crates/keel-opt/src/call_load_reuse.rs', root/'crates/keel-opt/tests/call_load_reuse.rs', fixture, driver, pathlib.Path(__file__)]:
        report['source_sha256'][str(p)] = digest(p)
    run('clang', '-O0', fixture, driver, '-o', out/'clang-oracle')
    run(out/'clang-oracle')
    for target in ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']:
        before, after, control_in, control = [out/f'{target}-{s}.keel' for s in ['before', 'after', 'leaf-only-input', 'leaf-only']]
        run(keel, 'cc', '--target', target, '--opt-tier', 'full', '--emit-ir', fixture, '-o', before)
        run(keel, 'opt', '--verify-each', '-p', 'call-load-reuse', before, '-o', after)
        control_in.write_text(ablate(before.read_text()))
        run(keel, 'opt', '--verify-each', '-p', 'call-load-reuse', control_in, '-o', control)
        old, new, leaf_only = map(lambda p: functions(p.read_text()), [before, after, control])
        counts = {n: dict(before=loads(old[n]), after=loads(new[n]), leaf_only=loads(leaf_only[n])) for n in old if n.startswith('effects_')}
        for name in ['effects_reuse', 'effects_spill']:
            assert counts[name]['before']-counts[name]['after'] == 1, counts
            assert counts[name]['leaf_only'] == counts[name]['before'], counts
        for name in ['effects_alias', 'effects_callback', 'effects_order']:
            assert counts[name] == dict(before=2, after=2, leaf_only=2), counts
        assert sum(b.count(' = call @') for b in old.values()) == sum(b.count(' = call @') for b in new.values())
        for name in counts:
            assert new[name].find(' = load.') < new[name].find(' = call @')
            assert re.findall(r'call @[^\n]+', old[name]) == re.findall(r'call @[^\n]+', new[name])
        direct, assembly, via = [out/f'{target}{s}' for s in ['-direct.o', '.s', '-text.o']]
        run(keel, 'compile', '--target', target, '--verify-each', after, '--emit-object', '-o', direct)
        run(keel, 'compile', '--target', target, '--verify-each', after, '-o', assembly)
        run(keel, 'asm', '--target', target, assembly, '-o', via)
        assert text_section(direct) == text_section(via)
        entry = dict(target=target, loads=counts, text_sha256=hashlib.sha256(text_section(direct)).hexdigest(),
                     assembly_stack_references=len(re.findall(r'\bsp\b|\brsp\b|\brbp\b', assembly.read_text())), native_executed=False)
        if target.startswith('aarch64'):
            for obj in [direct, via]:
                binary = obj.with_suffix('.oracle')
                run('clang', obj, driver, '-o', binary)
                run(binary)
            entry['native_executed'] = True
            entry['native_controls'] = ['201 scalar/alias/mutation/callback/spill cases', 'first read faults before marker mutation', 'second read faults after callback revokes page readability']
        report['targets'].append(entry)
        save()
    # Source locations continue to disable proof. Check the frontend debug route.
    debug_before, debug_after = out/'debug-before.keel', out/'debug-after.keel'
    run(keel, 'cc', '-g', '--opt-tier', 'full', '--emit-ir', fixture, '-o', debug_before)
    run(keel, 'opt', '--verify-each', '-p', 'call-load-reuse', debug_before, '-o', debug_after)
    # Parsing printed IR compacts sparse value numbers left by frontend cleanup.
    normalize = lambda p: re.sub(r"\bv[0-9]+\b", "v", p.read_text())
    assert normalize(debug_before) == normalize(debug_after)
    report['debug_ir_unchanged_modulo_value_numbering'] = True
    for n, original in enumerate(args.originals):
        before, ordinary, after, control_in, control = [out/f'app{n}-{s}.keel' for s in ['before', 'ordinary', 'after', 'leaf-only-input', 'leaf-only']]
        run(keel, 'cc', '--opt-tier', 'full', '--emit-ir', original, '-o', before)
        run(keel, 'opt', '--verify-each', '-p', 'load-reuse', before, '-o', ordinary)
        run(keel, 'opt', '--verify-each', '-p', 'call-load-reuse', ordinary, '-o', after)
        control_in.write_text(ablate(ordinary.read_text()))
        run(keel, 'opt', '--verify-each', '-p', 'call-load-reuse', control_in, '-o', control)
        old, new, leaf_only = map(lambda p: functions(p.read_text()), [ordinary, after, control])
        changes = {name: dict(before=loads(old[name]), after=loads(new[name]), leaf_only=loads(leaf_only[name])) for name in old if loads(old[name]) != loads(new[name])}
        report['original_c'].append(dict(source=str(original), sha256=digest(original), changes=changes,
                                         total_removed=sum(v['before']-v['after'] for v in changes.values()),
                                         transitive_summary_dependent_removed=sum(loads(leaf_only[n])-loads(new[n]) for n in new)))
        save()
    report['artifacts'] = {str(p): digest(p) for p in sorted(out.iterdir()) if p.is_file() and p.name not in ['report.html', 'report.json', 'lock.jsonl']}
    report['complete'] = True
    save()
    print(json.dumps(dict(targets=report['targets'], original_c=report['original_c'], complete=True), indent=2))


if __name__ == '__main__':
    main()
