#!/usr/bin/env python3
"""Run only under suite/fast_opt_locked.py. No performance measurements."""
import argparse
import hashlib
import html
import json
from pathlib import Path
import re
import shutil
import sys
import tarfile
import urllib.request

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(ROOT / 'suite'))
from cpp_coverage import Runner, Failed, classify_import, digest, SENTINEL


def oracle():
    rows, total = [], 0
    for r in range(64):
        ids = sum(range(r * 32, (r + 1) * 32))
        prices = sum(r * 32 + i + 1 for i in range(32) if i % 3 == 0)
        total += ids + prices
        rows.append(f'round={r} nodes=32 ids={ids} selected=11 prices={prices}\n')
    return (''.join(rows) + f'documents=64 nodes=2048 total={total}\n').encode()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', required=True)
    p.add_argument('--keel', required=True)
    p.add_argument('--libllvm', required=True)
    p.add_argument('--archive', help='optional predownloaded official archive')
    p.add_argument('--clangxx', default='clang++')
    a = p.parse_args()
    out = Path(a.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    run = Runner(out, 180)
    manifest = json.loads((HERE / 'manifest.json').read_text())
    report = dict(status='failed', stage='setup', commands=run.commands, modules=[],
                  manifest=manifest, tiers=[], scope='Correctness/import blocker pilot; no timing or native C++ frontend claim',
                  ir_preparation='Clang -O1 already optimized input, default exceptions/RTTI, untouched IR')
    code = 1
    try:
        shutil.copytree(HERE, out / 'pilot', ignore=shutil.ignore_patterns('__pycache__'))
        archive = out / 'pugixml-1.15.tar.gz'
        if a.archive:
            shutil.copyfile(a.archive, archive)
        else:
            urllib.request.urlretrieve(manifest['archive_url'], archive)
        if digest(archive) != manifest['archive_sha256'] or archive.stat().st_size != manifest['archive_size']:
            raise Failed('archive pin mismatch')
        report['archive_verified'] = True
        # Full original archive retained; extract only exact required sources/license.
        source = out / 'upstream'
        source.mkdir()
        with tarfile.open(archive) as tf:
            for name in [*manifest['source_sha256'], 'LICENSE.md']:
                data = tf.extractfile('pugixml-1.15/' + name).read()
                dest = source / name
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(data)
                if name in manifest['source_sha256'] and digest(dest) != manifest['source_sha256'][name]:
                    raise Failed('source pin mismatch: ' + name)
        for name in ['llvm_import.py', 'llvm_import_globals.py', 'llvm_import_linkage.py', 'cpp_coverage.py']:
            shutil.copyfile(ROOT / 'suite' / name, out / name)
        report['tool_hashes'] = {name: digest(out / name) for name in ['llvm_import.py', 'llvm_import_globals.py', 'llvm_import_linkage.py', 'cpp_coverage.py']}
        keel = Path(a.keel).resolve()
        report['keel'] = dict(path=str(keel), sha256=digest(keel))
        version = run.require([a.clangxx, '--version'])
        report['clang_version'] = Path(version['stdout']).read_text()
        revision = run.require(['git', 'rev-parse', 'HEAD'])
        report['revision'] = Path(revision['stdout']).read_text().strip()
        probe = run.require([sys.executable, '-c', 'import sys;sys.path.insert(0,sys.argv[1]);from llvm_import import LLVM;print(LLVM(sys.argv[2]).version)', str(out), a.libllvm])
        report['llvm_version'] = Path(probe['stdout']).read_text().strip()
        report['libllvm'] = dict(path=str(Path(a.libllvm).resolve()), sha256=digest(Path(a.libllvm)))
        originals = [('pugixml', source / 'src/pugixml.cpp'), ('driver', out / 'pilot/driver.cpp')]
        flags = ['-std=c++17', '-O1', '-I', str(source / 'src')]
        report['stage'] = 'clang_ir'
        for name, src in originals:
            ir = out / (name + '.ll')
            run.require([a.clangxx, *flags, '-S', '-emit-llvm', src, '-o', ir])
            text = ir.read_text()
            triple = re.search(r'^target triple = "(.*)"', text, re.M).group(1)
            if not re.match(r'(arm64|aarch64)-apple-', triple):
                raise Failed('requires Apple AArch64: ' + triple)
            report['modules'].append(dict(name=name, source_sha256=digest(src), ir_sha256=digest(ir),
                target=triple, data_layout=re.search(r'^target datalayout = "(.*)"', text, re.M).group(1), import_status='not_attempted'))
        report['stage'] = 'native_reference'
        # Direct source reference and exact-IR reference must both satisfy independent oracle.
        run.require([a.clangxx, *flags, *(src for _, src in originals), '-o', out / 'reference-source'])
        for name, _ in originals:
            run.require([a.clangxx, '-c', out / (name + '.ll'), '-o', out / (name + '.reference.o')])
        run.require([a.clangxx, *(out / (name + '.reference.o') for name, _ in originals), '-o', out / 'reference-ir'])
        expected = oracle()
        (out / 'expected.stdout').write_bytes(expected)
        report['references'] = []
        for kind in ['source', 'ir']:
            result = run.require([out / ('reference-' + kind)])
            if Path(result['stdout']).read_bytes() != expected:
                raise Failed('reference differs from independent oracle: ' + kind)
            report['references'].append(dict(kind=kind, status='passed', stdout=result['stdout'], sha256=hashlib.sha256(expected).hexdigest()))
        report['stage'] = 'import'
        for mod in report['modules']:
            name = mod['name']
            ir, dest = out / (name + '.ll'), out / (name + '.keel')
            dest.write_bytes(SENTINEL)
            result = run.run([sys.executable, out / 'llvm_import.py', '--libllvm', a.libllvm, '--keel', keel, ir, '-o', dest])
            stderr = Path(result['stderr']).read_text(errors='replace')
            mod.update(import_status=classify_import(result['returncode'], Path(result['stdout']).read_bytes(), stderr, dest.read_bytes() == SENTINEL), diagnostic=stderr, destination_preserved=dest.read_bytes() == SENTINEL)
            if digest(ir) != mod['ir_sha256']:
                raise Failed('input IR changed')
            if mod['import_status'] != 'accepted':
                report['status'] = 'blocked' if mod['import_status'] == 'rejected' else 'failed'
                report['blocker'] = stderr
                if mod['import_status'] == 'rejected' and 'i1 is internal only' in stderr:
                    # A separate minimal source, never a rewrite of upstream IR.
                    reduced = out / 'reduced-bool.ll'
                    run.require([a.clangxx, '-std=c++17', '-O1', '-S', '-emit-llvm', out / 'pilot/reduced_bool.cpp', '-o', reduced])
                    reduced_hash = digest(reduced)
                    run.require([a.clangxx, reduced, '-o', out / 'reduced-bool-reference'])
                    run.require([out / 'reduced-bool-reference'])
                    reduced_dest = out / 'reduced-bool.keel'
                    reduced_dest.write_bytes(SENTINEL)
                    rr = run.run([sys.executable, out / 'llvm_import.py', '--libllvm', a.libllvm, '--keel', keel, reduced, '-o', reduced_dest])
                    diagnostic = Path(rr['stderr']).read_text(errors='replace')
                    report['reduced_case'] = dict(source='pilot/reduced_bool.cpp', ir_sha256=reduced_hash,
                        native_reference='passed', diagnostic=diagnostic,
                        status=classify_import(rr['returncode'], Path(rr['stdout']).read_bytes(), diagnostic, reduced_dest.read_bytes() == SENTINEL))
                    if digest(reduced) != reduced_hash or report['reduced_case']['status'] != 'rejected':
                        raise Failed('reduced case did not reproduce rejection')
                raise Failed('import did not accept ' + name)
        report['stage'] = 'keel_differential'
        for tier in ['none', 'fast', 'full']:
            for name, _ in originals:
                run.require([keel, 'compile', '--opt-tier', tier, '--emit-object', out / (name + '.keel'), '-o', out / (name + '.' + tier + '.o')])
            exe = out / ('keel-' + tier)
            run.require([a.clangxx, *(out / (name + '.' + tier + '.o') for name, _ in originals), '-o', exe])
            result = run.require([exe])
            if Path(result['stdout']).read_bytes() != expected:
                raise Failed('Keel differential mismatch: ' + tier)
            report['tiers'].append(dict(tier=tier, status='passed'))
        report.update(status='passed', stage='complete')
        code = 0
    except Exception as e:
        report['error'] = str(e)
    finally:
        report['exit_code'] = code
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        summary = {key: report.get(key) for key in ['status', 'stage', 'ir_preparation', 'references', 'blocker', 'reduced_case', 'tiers', 'exit_code']}
        (out / 'report.html').write_text('<!doctype html><meta charset="utf-8"><style>body{font:15px system-ui;color:var(--foreground,#222)}pre{white-space:pre-wrap;overflow-wrap:anywhere}</style><h2>pugixml 1.15 importer pilot</h2><p>Correctness and rejection inventory. No performance measurements. None/Fast/Full require both modules to import.</p><pre>' + html.escape(json.dumps(summary, indent=2)) + '</pre><p>Full commands and source/tool identities: report.json</p>')
    return code


if __name__ == '__main__':
    sys.exit(main())
