#!/usr/bin/env python3
"""Run only through fast_opt_locked.py; standalone unregistered pass validation."""
import argparse
import hashlib
import html
import json
import pathlib
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--evidence-root', type=pathlib.Path)
    args = parser.parse_args()
    out = pathlib.Path('target/aggregate-return-20261021')
    out.mkdir(parents=True, exist_ok=True)
    report = {'base': '8d756b0a', 'scope': 'Standalone opt-in module; no registration, pipeline, ABI, allocator, defaults, remotes or performance measurements.', 'commands': [], 'evidence': []}

    def run(command):
        result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        report['commands'].append(dict(argv=command, returncode=result.returncode, stdout=result.stdout, stderr=result.stderr))
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        if result.returncode:
            raise RuntimeError(f'{command}: {result.stdout}\n{result.stderr}')
        print(result.stdout, end='', flush=True)
        return result.stdout

    cargo = ['cargo', 'test', '--offline', '--locked', '-j2']
    run(cargo + ['-p', 'keel-opt', '--test', 'aggregate_return', '--', '--test-threads=1'])
    run(cargo + ['-p', 'keel', '--test', 'aggregate_return', '--', '--test-threads=1'])
    if args.evidence_root:
        for tag, relative in [('full', 'target/arm-real-gap-20261021/quickjs-full.keel'), ('guarded', 'target/arm-real-implementation-20261021/static/quickjs-guarded.keel')]:
            source = args.evidence_root / relative
            destination = out / f'quickjs-{tag}-forwarded.keel'
            result = run(['cargo', 'run', '--offline', '--locked', '-j2', '-p', 'keel', '--example', 'aggregate_return_probe', '--', str(source), str(destination)])
            report['evidence'].append(dict(input=str(source), sha256=hashlib.sha256(source.read_bytes()).hexdigest(), result=json.loads(result), output=str(destination)))
    report['artifacts'] = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((out / 'native').glob('*')) if p.is_file()}
    report['native'] = 'Clang oracle and transformed direct/text objects executed on host ARM macOS; x86-64 Linux direct/text .text compared, not executed.'
    report['limits'] = ['Only local single-block write-only leaf constructors, 1..64 bytes, <=128 instructions and <=512 values.', 'Caller <=50000 instructions and <=100000 values; <=2000000 instruction/operand scan units, plus bounded 128-instruction windows; bail out conservatively.', 'Source address closure allows only the selected direct call, exact copy loads and constant in-bounds address derivation. Destination remains an independent exact-sized fixed slot; calls using it must follow the completed copy in the same block.', 'No generic nocapture assumption. No return-slot/parameter/heap forwarding. Debug, intrinsics, invoke, inline assembly and returns-twice excluded.', 'Static eligibility and functional correctness only; no runtime, frame-size, or profitability claim. Parent owns integration and independent review.']
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    body = '<!doctype html><meta charset="utf-8"><title>Aggregate result prototype</title><style>body{font:16px system-ui;max-width:1000px;margin:40px auto;line-height:1.5}pre{white-space:pre-wrap;background:#f2f4f7;padding:16px}</style><h1>Aggregate result-home prototype</h1><p>' + html.escape(report['scope']) + '</p><p>' + html.escape(report['native']) + '</p><h2>Retained actual C evidence</h2><pre>' + html.escape(json.dumps(report['evidence'], indent=2)) + '</pre><h2>Proof boundary</h2><ul>' + ''.join('<li>' + html.escape(x) + '</li>' for x in report['limits']) + '</ul><h2>Validation</h2><pre>' + html.escape(json.dumps(report['commands'], indent=2)) + '</pre>'
    (out / 'report.html').write_text(body)


if __name__ == '__main__':
    main()
