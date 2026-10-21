#!/usr/bin/env python3
"""Check ordered-loop activation and IR identity on actual C inputs.

Run under fast_opt_locked.py. This is an applicability check, not a runtime
benchmark. The kernel input is a positive control and is reported separately.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keel', required=True, type=Path)
    parser.add_argument('--inputs', required=True, type=Path,
                        help='prepared suite directory containing lua/pre, quickjs/pre, sqlite/pre')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    keel, inputs, output = args.keel.resolve(), args.inputs.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    sources = [('positive-control', root / 'suite/runtime-kernels/kernels.c')]
    for app in ['lua', 'quickjs', 'sqlite']:
        prepared = sorted((inputs / app / 'pre').glob('*.i'))
        if not prepared:
            raise RuntimeError(f'no prepared inputs for {app}')
        sources.extend((app, p) for p in prepared)
    report = dict(complete=False, errors=[], compiler=str(keel), compiler_sha256=digest(keel),
                  runner_sha256=digest(Path(__file__)), records=[], pipelines={},
                  method=__doc__)

    def save():
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

    def compile_ir(source, directory, name, passes=None):
        ir, observation = directory / (name + '.keel'), directory / (name + '.json')
        command = [str(keel), 'cc', '-O', '--emit-ir', '--opt-report', str(observation),
                   str(source), '-o', str(ir)]
        if passes is not None:
            command += ['-p', ','.join(passes)]
        result = subprocess.run(command, capture_output=True, text=True, timeout=600)
        with (output / 'commands.jsonl').open('a') as log:
            log.write(json.dumps(dict(argv=command, returncode=result.returncode,
                                      stdout=result.stdout, stderr=result.stderr)) + '\n')
        if result.returncode:
            raise RuntimeError(f'{command}: {result.stderr}')
        return dict(ir_sha256=digest(ir), report=observation, ir=ir)

    try:
        for app, source in sources:
            directory = output / app / source.stem
            directory.mkdir(parents=True)
            baseline = compile_ir(source, directory, 'baseline')
            observations = json.loads(baseline['report'].read_text())['invocations']
            indexed = {row['pipeline_index']: row['pass'] for row in observations}
            pipeline = [indexed[i] for i in sorted(indexed)]
            if not report['pipelines']:
                cleanup = ['canonicalize', 'instcombine', 'dce']
                report['pipelines'] = dict(
                    baseline=pipeline, cleanup=pipeline + cleanup,
                    wide=pipeline + ['adjacent-diff'] + cleanup,
                    state=pipeline + ['run-balance-predicate'] + cleanup,
                    both=pipeline + ['adjacent-diff', 'run-balance-predicate'] + cleanup)
            elif any(i >= len(report['pipelines']['baseline'])
                     or report['pipelines']['baseline'][i] != name
                     for i, name in indexed.items()):
                # Translation units without functions have no observations
                # for function passes, but still report module passes.
                raise RuntimeError('observed default pipeline differs from control')
            row = dict(app=app, source=str(source), input_sha256=digest(source), configurations={})
            for name in report['pipelines']:
                artifacts = baseline if name == 'baseline' else compile_ir(
                    source, directory, name, report['pipelines'][name])
                invocations = json.loads(artifacts['report'].read_text())['invocations']
                selected = [x for x in invocations if x['pass'] in
                            ['adjacent-diff', 'run-balance-predicate']]
                row['configurations'][name] = dict(
                    ir_sha256=artifacts['ir_sha256'],
                    activation=[dict(pass_name=x['pass'], function=x['function'],
                                     changed=x['changed'], pass_ns=x['pass_ns']) for x in selected],
                    changed_functions=sorted({x['function'] for x in selected if x['changed']}))
            control = row['configurations']['cleanup']['ir_sha256']
            for name in ['wide', 'state', 'both']:
                variant = row['configurations'][name]
                variant['identical_to_cleanup'] = variant['ir_sha256'] == control
                if not variant['changed_functions'] and not variant['identical_to_cleanup']:
                    raise RuntimeError(f'nonmatching passes changed IR for {source}: {name}')
            report['records'].append(row)
            save()
            print(app, source.name, {n: len(x.get('changed_functions', []))
                                    for n, x in row['configurations'].items()}, flush=True)
        positive = report['records'][0]['configurations']
        assert positive['wide']['changed_functions'], 'wide positive control did not activate'
        assert positive['state']['changed_functions'], 'state positive control did not activate'
        assert digest(keel) == report['compiler_sha256'], 'compiler changed during validation'
        report['complete'] = True
    except Exception as error:
        report['errors'].append(str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
