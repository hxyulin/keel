#!/usr/bin/env python3
"""Run only under fast_opt_locked.py; correctness/static evidence, no timings."""
import argparse
import hashlib
import html
import json
import pathlib
import re
import struct
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def functions(source):
    return {m[0]: m[1] for m in re.findall(r'^func @([^ (]+)(.*?^})', source, re.M | re.S)}


def loads(body):
    return len(re.findall(r' = load\.', body))


def text_section(path):
    data = path.read_bytes()
    if data[:4] == b'\xcf\xfa\xed\xfe':
        ncmds = struct.unpack_from('<I', data, 16)[0]
        pos = 32
        for _ in range(ncmds):
            cmd, size = struct.unpack_from('<II', data, pos)
            if cmd == 0x19:
                nsects = struct.unpack_from('<I', data, pos + 64)[0]
                for n in range(nsects):
                    section = pos + 72 + 80*n
                    if data[section:section+16].rstrip(b'\0') == b'__text':
                        length, offset = struct.unpack_from('<QI', data, section+40)
                        return data[offset:offset+length]
            pos += size
    elif data[:4] == b'\x7fELF':
        offset = struct.unpack_from('<Q', data, 40)[0]
        size, count, strings = struct.unpack_from('<HHH', data, 58)
        names_offset, names_size = struct.unpack_from('<QQ', data, offset+strings*size+24)
        names = data[names_offset:names_offset+names_size]
        for n in range(count):
            section = offset+n*size
            name = struct.unpack_from('<I', data, section)[0]
            if names[name:].split(b'\0', 1)[0] == b'.text':
                start, length = struct.unpack_from('<QQ', data, section+24)
                return data[start:start+length]
    raise ValueError(f'missing text: {path}')


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--keel', required=True, type=pathlib.Path)
    ap.add_argument('--evidence-root', required=True, type=pathlib.Path)
    ap.add_argument('--out', required=True, type=pathlib.Path)
    ap.add_argument('--skip-native', action='store_true')
    ap.add_argument('--original-tier', choices=['full', 'scalar'], default='full')
    ap.add_argument('--originals', nargs='*', default=['lua/pre/ltable.i', 'lua/pre/lstrlib.i', 'quickjs/pre/cutils.i'])
    args = ap.parse_args()
    root = pathlib.Path(__file__).resolve().parents[1]
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    keel = str(args.keel.resolve())
    report = dict(compiler=dict(path=keel, sha256=digest(pathlib.Path(keel))), source_sha256={str(root/p): digest(root/p) for p in ['crates/keel-opt/src/call_load_reuse.rs', 'crates/keel-opt/src/lib.rs', 'suite/call_load_reuse_validate.py', 'crates/keel/tests/fixtures/call_load_reuse.c', 'crates/keel/tests/fixtures/call_load_reuse_driver.c']}, base='bd47cc3451d2131bdaadf6f027c75959eac11696', pass_name='call-load-reuse', commands=[], native=[], original_c=[], artifacts={})

    def save():
        (out/'report.json').write_text(json.dumps(report, indent=2)+'\n')
        (out/'report.html').write_text('<!doctype html><meta charset="utf-8"><title>Call load reuse validation</title><style>body{font:16px system-ui;max-width:1100px;margin:36px auto}pre{white-space:pre-wrap}</style><h1>Call load reuse: correctness and static evidence</h1><pre>'+html.escape(json.dumps(report, indent=2))+'</pre>')

    def run(*command, env=None):
        command = list(map(str, command))
        result = subprocess.run(command, text=True, capture_output=True, env=env)
        report['commands'].append(dict(argv=command, returncode=result.returncode, stdout=result.stdout, stderr=result.stderr,
                                       environment_overrides={k:v for k,v in (env or {}).items() if k.startswith('DYLD_')}))
        save()
        if result.returncode:
            raise RuntimeError(f'{command}: {result.stdout}\n{result.stderr}')
        return result.stdout

    fixture = root/'crates/keel/tests/fixtures/call_load_reuse.c'
    driver = root/'crates/keel/tests/fixtures/call_load_reuse_driver.c'
    if not args.skip_native:
        run('clang', '-O0', fixture, driver, '-o', out/'clang-oracle')
        run(out/'clang-oracle')
    for target in ([] if args.skip_native else ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']):
        before, after = out/f'{target}-before.keel', out/f'{target}-after.keel'
        run(keel, 'cc', '--target', target, '--opt-tier', 'full', '--emit-ir', fixture, '-o', before)
        run(keel, 'opt', '--verify-each', '-p', 'call-load-reuse', before, '-o', after)
        old, new = functions(before.read_text()), functions(after.read_text())
        counts = {name: [loads(old[name]), loads(new[name])] for name in ['reuse', 'mutation', 'indirect', 'interposable']}
        assert counts['reuse'] == [2, 1], counts
        for name in ['mutation', 'indirect', 'interposable']:
            assert counts[name] == [2, 2], counts
        assert 'call @leaf' in new['reuse']
        assembly, direct, via = out/f'{target}.s', out/f'{target}-direct.o', out/f'{target}-text.o'
        run(keel, 'compile', '--target', target, '--verify-each', after, '--emit-object', '-o', direct)
        run(keel, 'compile', '--target', target, '--verify-each', after, '-o', assembly)
        run(keel, 'asm', '--target', target, assembly, '-o', via)
        assert text_section(direct) == text_section(via)
        entry = dict(target=target, loads=counts, text_sha256=hashlib.sha256(text_section(direct)).hexdigest(), native_executed=False)
        if target == 'aarch64-apple-darwin':
            definition = re.search(r'^func @replaceable[^\n]*\n.*?^}\n', after.read_text(), re.M | re.S).group(0)
            declaration = definition.splitlines()[0].replace('func @', 'decl @').removesuffix(' {')+'\n'
            wrapper_ir, definition_ir = out/'interpose-wrapper.keel', out/'interpose-definition.keel'
            wrapper_ir.write_text(after.read_text().replace(definition, declaration))
            definition_ir.write_text(definition)
            wrapper_obj, definition_obj = out/'interpose-wrapper.o', out/'interpose-definition.o'
            run(keel, 'compile', '--target', target, '--verify-each', wrapper_ir, '--emit-object', '-o', wrapper_obj)
            run(keel, 'compile', '--target', target, '--verify-each', definition_ir, '--emit-object', '-o', definition_obj)
            wrapper_s, wrapper_text = out/'interpose-wrapper.s', out/'interpose-wrapper-text.o'
            run(keel, 'compile', '--target', target, '--verify-each', wrapper_ir, '-o', wrapper_s)
            run(keel, 'asm', '--target', target, wrapper_s, '-o', wrapper_text)
            for obj, interpose_obj in [(direct, wrapper_obj), (via, wrapper_text)]:
                binary = obj.with_suffix('.oracle')
                run('clang', '-O2', obj, driver, '-o', binary)
                run(binary)
                # The optimizer saw the pure interposable definition. Emit its
                # caller separately and bind that symbol to a mutating definition.
                # This tests legal link-time replacement without relying on DYLD.
                replacement = out/'replacement.c'
                replacement.write_text('long replaceable(long *p, long x) { *p=x-9; return x+13; }\n')
                interposed = obj.with_suffix('.replacement-oracle')
                run('clang', '-O2', '-DINTERPOSED', driver, interpose_obj, replacement, '-o', interposed)
                run(interposed)
            entry['native_executed'] = True
            entry['interposition'] = 'Optimizer saw pure interposable body; optimized IR caller emitted separately and linked to a mutating replacement; both caller object routes passed. This is link-time substitution, not runtime DYLD interposition.'
        report['native'].append(entry)
        save()
    for relative in args.originals:
        source = (args.evidence_root/'target/suite'/relative).resolve()
        tag = relative.replace('/', '-').removesuffix('.i')
        before, control, after = out/f'{tag}-full.keel', out/f'{tag}-control.keel', out/f'{tag}-after.keel'
        cleanup = ['--opt-tier', 'full'] if args.original_tier == 'full' else ['-p', 'fast-cfg,fast-local,stack-forward,stack-promote,canonicalize,instcombine,dce,simplify-cfg,stack-forward,dce']
        run(keel, 'cc', *cleanup, '--emit-ir', source, '-o', before)
        run(keel, 'opt', '--verify-each', '-p', 'load-reuse', before, '-o', control)
        run(keel, 'opt', '--verify-each', '-p', 'call-load-reuse', control, '-o', after)
        old, new = functions(control.read_text()), functions(after.read_text())
        changes = {name: dict(before=loads(body), after=loads(new[name])) for name,body in old.items() if loads(body) != loads(new[name])}
        no_summary_input, no_summary_output = out/f'{tag}-unbound.keel', out/f'{tag}-unbound-after.keel'
        no_summary_input.write_text(re.sub(r'^(func @[^\n]*)$', lambda m: m[0].replace(' local', '').replace(' noninterposable', ''), control.read_text(), flags=re.M))
        run(keel, 'opt', '--verify-each', '-p', 'call-load-reuse', no_summary_input, '-o', no_summary_output)
        no_summary = functions(no_summary_output.read_text())
        summary_changes = {name: loads(no_summary[name])-loads(body) for name,body in new.items() if loads(no_summary[name]) != loads(body)}
        report['original_c'].append(dict(input=str(source), input_sha256=digest(source), cleanup=cleanup, before=str(control), after=str(after), changes=changes, summary_dependent_removed_loads=summary_changes,
                                         removed_loads=sum(v['before']-v['after'] for v in changes.values())))
        save()
    report['artifacts'] = {str(p): digest(p) for p in sorted(out.iterdir()) if p.is_file() and p.name not in ['report.json', 'report.html']}
    report['limitations'] = ['No timings or runtime performance claim.', 'x86 objects emitted and text compared; not executed.', 'No general capture analysis: accepted leaves cannot persist operands but can return derived values.', 'Original-C activation includes exact dominating-value joins; inspect retained before/after IR for site attribution.']
    save()
    print(json.dumps(dict(native=report['native'], original_c=report['original_c']), indent=2))


if __name__ == '__main__':
    main()
