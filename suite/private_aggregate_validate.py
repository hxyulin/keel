#!/usr/bin/env python3
"""Non-self-locking validation: run via fast_opt_locked.py, never directly."""
import hashlib
import html
import json
import pathlib
import platform
import re
import shutil
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / 'target/runtime-call-round2-20261022/aggregate'
OUT.mkdir(parents=True, exist_ok=True)
report = {'base': 'dafc74db4dedac2351eea2c04e07c7adce9a9504', 'commands': [],
          'host': platform.platform(), 'scope': 'Standalone opt-in API; no registry/default pipeline changes, push, remotes, x86-64 Linux execution, timing or promotion.'}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(argv):
    r = subprocess.run(argv, text=True, capture_output=True)
    report['commands'].append(dict(argv=argv, returncode=r.returncode, stdout=r.stdout, stderr=r.stderr))
    (OUT / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(r.stdout + r.stderr, end='', flush=True)
    if r.returncode:
        raise RuntimeError(argv)
    return r.stdout


cargo = ['cargo', 'test', '--offline', '--locked', '-j2']
run(cargo + ['-p', 'keel-opt', '--test', 'private_aggregate', '--test', 'internal_fast_calls', '--test', 'aggregate_return', '--', '--test-threads=1'])
run(cargo + ['-p', 'keel', '--test', 'private_aggregate', '--', '--test-threads=1'])
source = ROOT / 'target/fastcc-cost-investigation-20261022/quickjs.keel'
destination = OUT / 'quickjs-transport.keel'
result = run(['cargo', 'run', '--offline', '--locked', '-j2', '-p', 'keel', '--example', 'private_aggregate_probe', '--', str(source), str(destination)])
report['quickjs'] = dict(input=str(source), input_sha256=sha(source), result=json.loads(result), output_sha256=sha(destination))
original = source.read_text()
transformed = destination.read_text()
report['quickjs']['selected'] = []
for name in ['JS_CallInternal', 'JS_FreeValue']:
    pattern = r'^func @' + name + r'\(.*?^}'
    before = re.search(pattern, original, re.M | re.S).group()
    after = re.search(pattern, transformed, re.M | re.S).group()
    row = dict(name=name, before=before.splitlines()[0], after=after.splitlines()[0],
               signature_changed=before.splitlines()[0] != after.splitlines()[0], body_changed=before != after, intrinsics=re.findall(r'.*intrinsic.*', before))
    report['quickjs']['selected'].append(row)
    (OUT / f'{name}-before.keel').write_text(before + '\n')
    (OUT / f'{name}-transport.keel').write_text(after + '\n')
# Preserve retained app assembly, not just references into a removable worktree.
app = ROOT / 'target/private-leaf-clobbers-20261022/apps/quickjs/keel-full/quickjs.s'
assembly = app.read_text()
report['retained_app'] = dict(path=str(app), sha256=sha(app), functions=[])
for name in ['JS_CallInternal', 'JS_FreeValue']:
    match = re.search(r'^_' + name + r':.*?(?=^\s*\.globl |^_[A-Za-z0-9_]+:|\Z)', assembly, re.M | re.S)
    if match:
        path = OUT / f'{name}-retained.s'
        path.write_text(match.group())
        report['retained_app']['functions'].append(dict(name=name, sha256=sha(path)))
clean = OUT / 'quickjs-selected-cleanup.keel'
run(['cargo', 'run', '--offline', '--locked', '-j2', '-p', 'keel', '--example', 'private_aggregate_probe', '--', str(source), str(clean), 'selected-cleanup'])
cleaned = clean.read_text()
for row in report['quickjs']['selected']:
    name = row['name']
    pattern = r'^func @' + name + r'\(.*?^}'
    before = re.search(pattern, original, re.M | re.S).group()
    after = re.search(pattern, cleaned, re.M | re.S).group()
    def counts(text):
        return dict(loads=len(re.findall(r' = load\.', text)), stores=len(re.findall(r'^\s*store ', text, re.M)))
    row['before_memory'] = counts(before)
    row['selected_cleanup_memory'] = counts(after)
    (OUT / f'{name}-selected-cleanup.keel').write_text(after + '\n')
shutil.copytree('target/private-aggregate-evidence/native', OUT / 'native', dirs_exist_ok=True)
report['native_commands'] = [json.loads(line) for line in (OUT / 'native/commands.jsonl').read_text().splitlines()]
report['native'] = 'ARM macOS: Clang reference plus raw transport and cleaned transport, direct and assembly-text objects linked and executed for x=-100..100. x86-64 Linux: both stages compile and direct/text .text bytes agree; not executed.'
report['tests'] = ['Independent per-argument copies, including identical source pointers and mutations through another pointer.', 'Captured callee home mutated during its lifetime; recursion and unchanged sret result contract.', 'Ten byval pairs force argument stack transport; both native object routes at both stages.', 'Public and address-taken signatures remain unchanged; padding, other sizes, weak linkage, tail calls, returns-twice, restrict contracts, assembly, intrinsics and debug exclusions.']
report['limits'] = ['Only exact [2 x i64] and {i64,i64}; no padding, other scalar types, layout flattening or sret optimization.', 'Caller loads promise only align-1; no source alignment is inferred from the layout. Each snapshot is reconstructed in its own 16-byte, align-8 home. This preserves alias, capture and mutation semantics; Exact i64 reads in the first 128 entry instructions forward from incoming words only until the first write/call/barrier; later accesses retain their homes. Downstream stack cleanup may remove provably unused homes. No profitability or speed claim.', 'C convention and all unsplit contracts remain unchanged. A separate opt-in internal-fast-calls run may promote eligible all-normal signatures. No registration is supplied automatically.', 'Public/weak/address-taken, variadic, returns-twice, debug, restrict, entry-backedge, intrinsic, atomic, variadic-access and over-aligned-access (>8) definitions excluded. Textual inline assembly or any returns-twice ordinary/indirect call disables the module.', 'Sret is preserved; JS_CallInternal has stack_alloc and its own signature is therefore deferred (its contained outgoing calls are still rewritten). JS_FreeValue qualifies. Whole-app static eligibility is not whole-app runtime validation.', 'Bounds: <=50000 instructions and <=100000 values per function, <=2000000 module instruction/operand units, <=32 original parameters, <=4096 inserted instructions per function and <=100000 module growth; conservative transactional bailout. Vec insertion still has bounded quadratic cost.', 'Only ARM native execution. x86 object consistency is not an execution oracle. Registration, integration, promotion and paired timing are out of scope.']
report['implementation_sources'] = {p: sha(pathlib.Path(p)) for p in ['crates/keel-opt/src/private_aggregate.rs', 'crates/keel-opt/tests/private_aggregate.rs', 'crates/keel/tests/private_aggregate.rs', 'crates/keel/tests/fixtures/private_aggregate.c', 'crates/keel/examples/private_aggregate_probe.rs', 'suite/private_aggregate_validate.py']}
report['artifacts'] = {str(p.relative_to(OUT)): sha(p) for p in sorted(OUT.rglob('*')) if p.is_file() and p.name not in ['report.json', 'report.html', 'lock.jsonl']}
(OUT / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
body = '<!doctype html><meta charset="utf-8"><title>Private aggregate transport</title><style>body{font:16px system-ui;max-width:1100px;margin:40px auto;line-height:1.5}pre{white-space:pre-wrap;background:#f2f4f7;padding:16px}</style><h1>Private two-word aggregate transport</h1><p>' + html.escape(report['scope']) + '</p><p>' + html.escape(report['native']) + '</p><h2>Actual retained C activation</h2><pre>' + html.escape(json.dumps(report['quickjs'], indent=2)) + '</pre><h2>Proof and limitations</h2><ul>' + ''.join('<li>' + html.escape(x) + '</li>' for x in report['limits']) + '</ul><h2>Commands and validation</h2><pre>' + html.escape(json.dumps(report['commands'], indent=2)) + '</pre>'
(OUT / 'report.html').write_text(body)
