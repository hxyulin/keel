"""Leased-host, untimed parity/count validation of the committed copied-source probe.

Builds are explicit separate commands. This runner never times or profiles code.
Pass the private directory containing fresh base/diagnostic sources and targets.
"""
import ast
import collections
import gzip
import hashlib
import json
import pathlib
import subprocess
import sys


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main(root):
    evidence = root / 'evidence'
    outputs = root / 'outputs'
    outputs.mkdir()
    fixtures = root / 'fixtures'
    fixtures.mkdir()
    cases = {
        'declaration': 'int f(void) { int = 3; return 0; }',
        'expression': 'int f(void) { return 1 + ; }',
        'cast': 'int f(void) { return (int *)1.5; }',
        'typedef': 'typedef int T; int f(void) { int T; T x; return 0; }',
        'initializer': 'int a[1] = { 1, 2 };',
        'cleanup': 'typedef struct S { int a; } S; int f(void) { S s = {1}; int *p = 3.5; return s.a; }',
        'lex': 'int f(void) { return "unterminated; }',
        'mapped': '# 37 "mapped-error.c"\nint f(void) { return unknown_name; }',
        'valid-shadow': 'typedef int T; int f(void) { T x=2; { int T=3; x*=T; } return (T)x; }',
        'valid-range': 'struct S { int a[4]; }; struct S s = { .a = { [1 ... 3] = 7 } };',
        'valid-cast': 'int f(int x) { return (int)(double)x + (x ? 1 : 2); }',
    }
    for name, source in cases.items():
        (fixtures / (name + '.i')).write_text(source + '\n')
    manifest = json.loads((root/'keel-parse-review-5655-diag/parse-probe-manifest.json').read_text())
    labels = manifest['labels']
    binaries = {'base': root/'target-base/release/examples/compile_driver',
                'diag': root/'target-diag/release/examples/parse_counts'}
    result = {'binaries': {k: {'path': str(p), 'sha256': sha(p.read_bytes())} for k,p in binaries.items()},
              'runner_sha256': sha(pathlib.Path(__file__).read_bytes()), 'rows': [], 'commands': [], 'aggregate': {}}
    (evidence/'generated-manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    corpus = sorted((root/'inputs').glob('*/pre/*.i'))
    assert len(corpus) == 44, len(corpus)
    jobs = [('corpus', p, 'x86_64', debug) for p in corpus for debug in [False,True]]
    jobs += [('fixture', p, target, debug) for p in sorted(fixtures.glob('*.i'))
             for target in ['x86_64','aarch64'] for debug in [False,True]]
    for index, (kind, source, target, debug) in enumerate(jobs):
        row = {'kind':kind, 'input':str(source), 'input_sha256':sha(source.read_bytes()),
               'target':target, 'debug':debug, 'outputs':{}, 'counts':[]}
        content = []
        for run, mode in enumerate(['base', 'diag', 'diag']):
            output = outputs / f'{index:03}-{mode}-{run}.txt'
            command = [str(binaries[mode]),target,str(debug).lower(),str(source),str(output)]
            proc = subprocess.run(command, capture_output=True)
            result['commands'].append({'argv':command,'returncode':proc.returncode,
                'stdout':proc.stdout.decode(),'stderr':proc.stderr.decode()})
            (evidence/'partial-results.json').write_text(json.dumps(result, indent=2)+'\n')
            assert proc.returncode == 0, (command, proc.stderr)
            data = output.read_bytes()
            content.append(data)
            row['outputs'][f'{mode}-{run}'] = {'path':str(output), 'bytes':len(data), 'sha256':sha(data)}
            if mode == 'diag':
                counts = ast.literal_eval(proc.stdout.decode())
                assert len(counts) == 4 and all(len(x) == len(labels) for x in counts)
                row['counts'].append(counts)
        assert content[0] == content[1] == content[2], row
        assert row['counts'][0] == row['counts'][1], row
        diagnostic = content[0].startswith(b'Diagnostic {')
        row['diagnostic'] = content[0].decode() if diagnostic else None
        if kind == 'fixture':
            assert diagnostic == (not source.stem.startswith('valid-')), (source, content[0][:200])
        else:
            assert not diagnostic, (source, content[0])
            key = source.parents[1].name + ':' + str(debug).lower()
            if key not in result['aggregate']:
                result['aggregate'][key] = {phase:dict.fromkeys(labels,0) for phase in manifest['phases']}
            for phase, counts in zip(manifest['phases'], row['counts'][0]):
                for label, count in zip(labels, counts):
                    result['aggregate'][key][phase][label] += count
        result['rows'].append(row)
    result['checks'] = {'corpus_parity':88, 'fixture_parity':len(jobs)-88,
                        'independent_count_captures':2, 'all_counter_rows_deterministic':True}
    with gzip.open(evidence/'validation.json.gz','wt') as f:
        json.dump(result,f,indent=2,sort_keys=True)
    (evidence/'summary.json').write_text(json.dumps({'checks':result['checks'],'aggregate':result['aggregate']},indent=2)+'\n')
    print(json.dumps(result['checks']))


if __name__ == '__main__':
    main(pathlib.Path(sys.argv[1]).resolve())
