"""Local-only independent evidence checks. Never invokes a compiler or remote tool.
Archive members are streamed or read by name; no archive paths are extracted.
"""
import collections
import gzip
import hashlib
import io
import importlib
import json
import pathlib
import random
import re
import statistics
import subprocess
import sys
import tarfile
import tempfile

BASE = '9eb96be6d6e9d87e4f860c27947694b227bdb857'
CANDIDATE = 'eb0d2bc92080274fd2b73ff45d75d9ef879d571a'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def stats(xs):
    med = statistics.median(xs)
    rng = random.Random(20261016)
    draws = sorted(statistics.median(rng.choices(xs, k=len(xs))) for _ in range(10000))
    return {'samples': xs, 'n': len(xs), 'median': med, 'min': min(xs), 'max': max(xs),
            'mad': statistics.median(abs(x-med) for x in xs),
            'median_ci95': [draws[249], draws[9749]]}


def main(root):
    p = json.loads((root/'provenance.json').read_text())
    seal = json.loads((root/'seal.json').read_text())
    summary = json.loads((root/'summary.json').read_text())
    decoded = json.loads(gzip.decompress((root/'decoded.json.gz').read_bytes()))
    out = {'candidate': CANDIDATE, 'base': BASE, 'archive_hashes': {},
           'headline': {}, 'profiles': {}, 'checks': {}, 'snapshots': {}}
    storage_manifest = json.loads((root/'storage-manifest.json').read_text())
    packed = (root/storage_manifest['stored_path']).read_bytes()
    original = gzip.decompress(packed)
    assert digest(packed) == storage_manifest['stored_sha256']
    assert digest(original) == storage_manifest['original_sha256']
    assert len(packed) == storage_manifest['stored_bytes'] and len(original) == storage_manifest['original_bytes']
    assert digest((root/'storage-manifest.json').read_bytes()) == seal['supplemental_storage_manifest']['sha256']
    assert original == subprocess.check_output(['git','show',CANDIDATE+':'+storage_manifest['original_path']])
    out['checks']['preflight_lossless_storage'] = True
    for name, key in [('raw-evidence.tar.gz', 'raw_archive_sha256'),
                      ('corpus.tar.gz', 'corpus_archive_sha256')]:
        h = digest((root/name).read_bytes())
        assert h == seal[key]
        out['archive_hashes'][name] = h
    git = lambda *args: subprocess.check_output(['git', *args])
    trees = {rev: git('rev-parse', rev+':crates').decode().strip()
             for rev in [BASE, '6e6d2c2', CANDIDATE, '21d0504', 'HEAD']}
    assert len(set(trees.values())) == 1
    out['compiler_crates_trees'] = trees
    production = json.loads((root/'production-compiler.json').read_text())
    prior = json.loads((root.parent/'functionattrs-benchmark-2026-10-16/linux-x86_64/provenance.json').read_text())
    assert production['after']['sha256'] == p['production_binary']['sha256']
    assert production['after']['sha256'] in json.dumps(prior['compilers'])
    out['production_sha256'] = production['after']['sha256']
    with tarfile.open(root/'raw-evidence.tar.gz') as raw:
        read = lambda name: raw.extractfile(name).read()
        load = lambda name: json.loads(read('evidence/'+name+'.json'))
        # Check all source bytes against the Git base, not only a recorded assertion.
        with tarfile.open(fileobj=io.BytesIO(read('source.tar.gz'))) as src:
            source = {m.name: src.extractfile(m).read() for m in src if m.isfile()}
        assert {n: digest(b) for n,b in source.items()} == p['source_pristine']['expected_sha256']
        batch = subprocess.Popen(['git', 'cat-file', '--batch'], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        for name, data in source.items():
            batch.stdin.write((BASE+':'+name+'\n').encode()); batch.stdin.flush()
            header = batch.stdout.readline().split()
            assert len(header) == 3, (name, header)
            assert batch.stdout.read(int(header[2])) == data, name
            assert batch.stdout.read(1) == b'\n'
        batch.stdin.close(); assert batch.wait() == 0
        out['checks']['source_files_identical_to_git_base'] = len(source)
        # Retained diagnostic manifests must change precisely the declared files.
        expected_changes = {'diagnostic-source': ['crates/keel-c/src/init.rs',
            'crates/keel-c/examples/profile_allocations.rs', 'crates/keel/src/main.rs',
            'crates/keel-opt/src/lib.rs'], 'storage-source': ['crates/keel-ir/src/function.rs',
            'crates/keel-c/src/strings.rs'], 'initializer-total-source': ['crates/keel-c/src/init.rs']}
        out['diagnostic_changed_files'] = {}
        for name, expected in expected_changes.items():
            manifest = load(name)['files']
            changed = sorted(n for n,h in manifest.items() if n in source and digest(source[n]) != h)
            assert changed == sorted(expected), (name, changed)
            out['diagnostic_changed_files'][name] = changed
        # Recreate only generated text in a private local temporary directory.
        # No build/profile/collection entry points are called.
        with tempfile.TemporaryDirectory(prefix='keel-fe-evidence-review-') as temp:
            task = pathlib.Path(temp)
            for name,data in source.items():
                relative = pathlib.PurePosixPath(name)
                assert not relative.is_absolute() and '..' not in relative.parts
                dest = task/'source'/name
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(data)
            sys.path.insert(0,str(pathlib.Path(__file__).resolve().parent))
            runner = importlib.import_module('profile_frontend_refresh')
            runner.ROOT = task
            diagnostic = importlib.import_module('diagnose_frontend_refresh')
            storage = importlib.import_module('count_frontend_storage')
            allocation = importlib.import_module('count_preprocessor_allocations')
            initializer = importlib.import_module('measure_initializer_storage')
            runner.setup()
            diagnostic.memory_setup()
            diagnostic.setup()
            storage.setup()
            allocation.setup()
            initializer.setup()
            for name in expected_changes:
                generated = {str(f.relative_to(task/name)):digest(f.read_bytes())
                             for f in (task/name).rglob('*.rs')}
                assert generated == load(name)['files'], name
            out['checks']['diagnostic_generators_reproduce_complete_manifests'] = 3
        # Corpus bytes against independent manifest, including all 44 raw C TUs.
        with tarfile.open(root/'corpus.tar.gz') as corpus:
            members = {m.name: {'sha256': digest(corpus.extractfile(m).read()), 'bytes': m.size}
                       for m in corpus if m.isfile()}
        expected = {n:v for group in p['input_manifest'].values() for n,v in group.items()}
        assert members == expected
        out['checks']['corpus_manifest_files'] = len(members)
        resident, cpp, cli = load('resident'), load('preprocess'), load('cli')
        assert cli['sha256'] == out['production_sha256']
        for mode,target in [('baseline','plain'),('staged','stage')]:
            assert resident['binaries'][mode]['sha256'] in p['binaries'][target].values()
        assert len(resident['samples']) == 156 and len(cpp['samples']) == 39 and len(cli['samples']) == 99
        out['checks']['timing_cohorts'] = [156,39,99]
        for app, count in [('lua',33),('quickjs',9),('sqlite',2)]:
            for debug in [False, True]:
                key = f'{app}:{debug}'
                totals = {}
                for mode in ['baseline', 'staged']:
                    rows = [x for x in resident['samples'] if (x['app'], x['debug'], x['mode']) == (app,debug,mode)]
                    assert [x['round'] for x in rows] == list(range(13))
                    assert all(len(x['rows']) == count for x in rows)
                    for field in ['total_ms','module_drop_ms'] + (['lex_ms','parse_ms','irgen_ms','cleanup_ms'] if mode == 'staged' else []):
                        values = [sum(y[field] for y in x['rows']) for x in rows]
                        result = stats(values)
                        assert result == summary['resident'][key][mode][field], (key,mode,field)
                        out['headline'][f'{key}:{mode}:{field}'] = result
                        if field == 'total_ms': totals[mode] = values
                    if mode == 'staged':
                        for tu in summary['per_tu'][key]:
                            samples = [y for x in rows for y in x['rows'] if y['path'] == tu['path']]
                            assert len(samples) == 13
                            assert all(y[k] == tu[k] for y in samples for k in ['bytes','tokens','functions'])
                            for field in ['total_ms','module_drop_ms','lex_ms','parse_ms','irgen_ms','cleanup_ms']:
                                values = [y[field] for y in samples]
                                med = statistics.median(values)
                                assert values == tu[field]['samples'] and med == tu[field]['median']
                                assert statistics.median(abs(x-med) for x in values) == tu[field]['mad']
                ratio = stats([a/b for a,b in zip(totals['staged'],totals['baseline'])])
                assert ratio == summary['resident'][key]['staged_plain_paired_ratio']
            for mode in ['pre-ir','raw-ir','raw-fullopt']:
                rows = [x for x in cli['samples'] if (x['app'],x['mode']) == (app,mode)]
                assert [x['round'] for x in rows] == list(range(11))
                assert all(len(x['rows']) == count for x in rows)
                assert all(x['total_ms'] == sum(y['wall_ms'] for y in x['rows']) for x in rows)
                result = stats([x['total_ms'] for x in rows])
                assert result == summary['cli'][app+':'+mode]
                out['headline'][app+':'+mode] = result
            rows = [x for x in cpp['samples'] if x['app'] == app]
            assert [x['round'] for x in rows] == list(range(13))
            assert all(len(x['rows']) == count for x in rows)
            result = stats([sum(y['preprocess_ms'] for y in x['rows']) for x in rows])
            assert result == summary['preprocess'][app]
            out['headline'][app+':preprocess'] = result
        # Reparse every raw perf event independently and preserve the denominator.
        for key, saved in decoded.items():
            name = key.replace(':','-')
            counts = collections.Counter(); selfw = collections.Counter(); incw = collections.Counter()
            folds = collections.Counter(); periods = []; block = []
            helper = 'preprocess_linux::main' if name.startswith('preprocess-') else 'profile::main'
            def event(lines):
                if not lines: return
                match = re.search(r':\s+(\d+)\s+cycles:u:\s*$',lines[0]); assert match, lines[0]
                w = int(match[1]); frames = []
                for line in lines[1:]:
                    frame = re.fullmatch(r'\s*[0-9a-f]+ (.+) \((.*)\)\s*',line); assert frame, line
                    frames.append(frame[1])
                periods.append(w); counts['samples'] += 1; counts['weight'] += w
                leaf = frames[0] if frames else '[no unwind frames]'
                selfw[leaf] += w
                for frame in set(frames): incw[frame] += w
                folds[';'.join(frames) if frames else '[no unwind frames]'] += w
                for label, bad in [('empty_chain', not frames), ('unknown_leaf',not frames or '[unknown]' in leaf),
                    ('any_unknown_frame',any('[unknown]' in f for f in frames)), ('does_not_reach_helper_main',helper not in frames)]:
                    if bad: counts[label+'_samples'] += 1; counts[label+'_weight'] += w
            for line in io.TextIOWrapper(raw.extractfile('evidence/'+name+'.perf.script.txt')):
                if line.strip(): block.append(line.rstrip('\n'))
                else: event(block); block = []
            event(block)
            assert dict(counts) == saved['totals'] and periods == saved['periods'], key
            assert dict(selfw) == dict(saved['self_weight']) and dict(incw) == dict(saved['inclusive_weight'])
            assert dict(folds) == dict(saved['folded_weight'])
            record = load(name+'-record')
            assert int(re.search(r'\((\d+) samples\)',record['stderr'])[1]) == counts['samples']
            # Independent scan of original perf's raw record dump, no dropped records.
            audit_name = 'evidence/'+name+'.perf.raw.txt.gz'
            lost = 0
            raw_samples = 0
            with gzip.GzipFile(fileobj=raw.extractfile(audit_name)) as stream:
                for line in stream:
                    lost += int(b'PERF_RECORD_LOST' in line or b'LOST_SAMPLES' in line)
                    raw_samples += int(b': PERF_RECORD_SAMPLE' in line)
            assert lost == 0 and raw_samples == counts['samples'], (key, raw_samples)
            audit = p['perf_loss_audit'][name+'.perf.data']
            assert digest(read('evidence/'+name+'.perf.data')) == audit['raw_sha256']
            assert digest(read(audit_name)) == audit['audit_sha256']
            out['profiles'][key] = {**dict(counts), 'lost_records':lost,
                'independent_raw_sample_records':raw_samples,
                'unknown_leaf_pct':100*counts['unknown_leaf_weight']/counts['weight'],
                'incomplete_chain_pct':100*counts['does_not_reach_helper_main_weight']/counts['weight'],
                'top_self':selfw.most_common(12),
                'insert_inst_inclusive_pct':sum(w for n,w in incw.items() if n == 'keel_ir::function::Function::insert_inst')/counts['weight']*100}
        # Manifest IR equality is local; large snapshots themselves are not local.
        ir = collections.defaultdict(dict)
        for name,h in p['ir_manifest'].items():
            path = pathlib.PurePosixPath(name)
            ir[(path.parts[2], path.name)][path.parts[3]] = h
        assert len(ir) == 88
        assert all(len(v) == 6 and len(set(v.values())) == 1 for v in ir.values())
        assert len(p['preprocessor_output_parity']) == 44 and all(x['equal'] and x['production_sha256'] == x['helper_sha256'] and x['returncode'] == 0 for x in p['preprocessor_output_parity'])
        out['checks']['ir_cases_with_six_matching_hashes'] = len(ir)
        out['checks']['native_cpp_parity_records'] = 44
        assert read('evidence/context.s') == read('evidence/original.s')
        out['checks']['locally_retained_assembly_pair_equal'] = True
        for name in ['allocations','preprocess-allocations']:
            runs = load(name)['runs']
            iterator = runs.items() if isinstance(runs,dict) else [(f"{x['app']}:{x['debug']}",x) for x in runs]
            for key,x in iterator:
                rows = [json.loads(line) for line in x['stdout'].splitlines()]
                fields = summary[name.replace('-','_')][key]
                assert fields == {k:sum(y[k] for y in rows) for k in fields}
        out['initializer'] = {}
        for x in load('initializer-total')['runs']:
            fields = dict((k,int(v)) for k,v in re.findall(r'(\w+)=(\d+)',x['stderr']))
            out['initializer'][f"{x['app']}:{x['debug']}"] = {**fields,'compile_passes':3,'clone_and_drop_ms_per_compile':fields['inclusive_ns']/3e6}
        out['storage'] = {}
        for x in load('storage')['runs']:
            fields = dict((k,int(v)) for k,v in re.findall(r'(\w+)=(\d+)',x['stderr']))
            out['storage'][f"{x['app']}:{x['debug']}"] = {k:(v/3 if k not in ['type_size','multi_result'] else v) for k,v in fields.items()}
        tests = read('tests.log').decode()
        arrays = re.findall(r'  left: (\[[^\n]*\])\n right: (\[[^\n]*\])',tests)
        assert len(arrays) == 12
        for left,right in arrays:
            normalize = lambda s:[re.split(r'\s+(?:Loc=<|\[)',x,maxsplit=1)[0] for x in json.loads(s)]
            assert normalize(left) == normalize(right)
        out['checks']['token_metadata_only_failures'] = 12
        out['checks']['per_tu_normal_debug_distributions'] = 88
        out['test_results'] = re.findall(r'test result:.*',tests+read('tests-run.log').decode()+read('tests-ssa.log').decode())
        for name in ['resident','cli','preprocess','allocations','samples','preprocess-samples','memory','diagnostics','storage','initializer-total']:
            d = load(name)
            out['snapshots'][name] = {k:{label:d[k][label]['stdout'].strip() for label in ['time','affinity','governor','load']} for k in ['start','end']}
        # Whole-pass and verification are independently bounded; do not subtract medians for an A/B claim.
        diagnostics = load('diagnostics')['cli_phases']
        out['call_contracts'] = {}
        for app in ['lua','quickjs','sqlite']:
            for phase in ['preprocess','frontend','verify','optimize','backend_output']:
                values = [sum(int(re.search(r'PHASE '+phase+r'_ns (\d+)',x['stderr'])[1]) for x in diagnostics if x['app']==app and x['round']==i)/1e6 for i in range(11)]
                # Summation after conversion matches the retained phase reducer.
                saved_values = [sum(int(re.search(r'PHASE '+phase+r'_ns (\d+)',x['stderr'])[1])/1e6 for x in diagnostics if x['app']==app and x['round']==i) for i in range(11)]
                assert all(abs(a-b)<1e-9 for a,b in zip(values,saved_values))
                assert stats(saved_values) == summary['cli_diagnostic_context'][app][phase+'_ms']
            result = {}
            for label,column in [('total',0),('verify',1)]:
                values = [sum(int(m[column]) for x in diagnostics if x['app']==app and x['round']==i for m in re.findall(r'OPT_CONTEXT call-contracts total_ns=(\d+) verify_ns=(\d+)',x['stderr']))/1e6 for i in range(11)]
                result[label] = stats(values)
                assert result[label] == summary['cli_diagnostic_context'][app]['call_contracts_'+label+'_ms']
            out['call_contracts'][app] = result
    (root/'independent-review.json').write_text(json.dumps(out,indent=2)+'\n')
    print('PASS: local archive/source/corpus, timing medians/MAD/CIs, 12 complete weighted profiles, IR manifests, allocations and test failure audit')


if __name__ == '__main__':
    main(pathlib.Path(sys.argv[1]))
