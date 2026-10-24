#!/usr/bin/env python3
"""Finite basic-profile evidence; run under fast_opt_locked.py with a built keel.

Usage: measure.py /absolute/path/to/keel /tmp/unique-evidence-dir
No build, install, cache changes, thresholds, or application-performance verdict.
"""
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import time


def leb(data, pos):
    value = shift = 0
    while True:
        byte = data[pos]
        pos += 1
        value |= (byte & 127) << shift
        if byte < 128:
            return value, pos
        shift += 7


def counts(data):
    # Decode only the integer MVP emitted by this corpus; unknown opcodes fail
    # rather than silently treating immediate bytes as extra instructions.
    assert data[:8] == b'\0asm\x01\0\0\0'
    pos = 8
    result = dict(module_bytes=len(data), code_body_bytes=0, locals=0, instructions=0)
    while pos < len(data):
        section = data[pos]
        size, pos = leb(data, pos + 1)
        end = pos + size
        if section == 10:
            functions, pos = leb(data, pos)
            for _ in range(functions):
                size, pos = leb(data, pos)
                body_end = pos + size
                result['code_body_bytes'] += size
                groups, pos = leb(data, pos)
                for _ in range(groups):
                    count, pos = leb(data, pos)
                    result['locals'] += count
                    pos += 1  # value type
                while pos < body_end:
                    opcode = data[pos]
                    pos += 1
                    result['instructions'] += 1
                    if opcode in (2, 3, 4, 12, 13, 16, 32, 33, 34, 35, 36, 65, 66):
                        _, pos = leb(data, pos)
                    elif opcode == 14:
                        entries, pos = leb(data, pos)
                        for _ in range(entries + 1):
                            _, pos = leb(data, pos)
                    elif 0x28 <= opcode <= 0x3e:
                        _, pos = leb(data, pos)
                        _, pos = leb(data, pos)
                    elif opcode in (0x3f, 0x40):
                        assert data[pos] == 0
                        pos += 1
                    elif opcode in (0, 1, 5, 11, 15, 26, 27) or 0x45 <= opcode <= 0x5a or 0x67 <= opcode <= 0x8a or opcode in (0xa7, 0xac, 0xad):
                        pass
                    else:
                        raise ValueError(f'unexpected opcode {opcode:#x}')
                assert pos == body_end
        pos = end
    return result


RUNTIME = r"""
const fs=require('fs'),{performance}=require('perf_hooks');
const b=fs.readFileSync(process.argv[1]);if(!WebAssembly.validate(b))throw Error('invalid');
const e=new WebAssembly.Instance(new WebAssembly.Module(b)).exports;
const isC=process.argv[2]==='cc';let sink=0n;
function run(){if(isC){for(let i=0;i<10000;i++)sink+=BigInt(e.checksum(8192,32)>>>0);}else{for(let i=0;i<10000;i++)sink+=e.fold(BigInt(i));}}
run();const samples=[];for(let i=0;i<5;i++){const t=performance.now();run();samples.push(performance.now()-t);}
console.log(JSON.stringify({runtime_batch_ms:samples,iterations_per_batch:10000,sink:sink.toString(),node:process.version}));
"""


def main():
    binary = pathlib.Path(sys.argv[1]).resolve()
    output = pathlib.Path(sys.argv[2])
    assert output.is_absolute() and output.parts[1] in ('tmp', 'private'), 'use /tmp'
    output.mkdir(exist_ok=False)
    corpus = pathlib.Path(__file__).resolve().parent
    load = os.getloadavg()
    cpus = os.cpu_count() or 1
    if load[0] > max(1, cpus // 2):
        raise SystemExit(f'host not quiet enough for finite samples: load={load}, cpus={cpus}')
    report = {'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'load_before': load, 'logical_cpus': cpus, 'note': 'Finite samples include process startup for CLI compile; runtime batches include JS/BigInt overhead. No gain claim.', 'corpus': {}}
    for kind, suffix, names in [('cc', 'c', ['checksum', 'local_array', 'recurse']), ('compile', 'keel', ['roundtrip', 'wrap', 'fold', 'divide', 'fault', 'hint', 'ordered', 'dead_div', 'dead_overflow', 'wrapread', 'frame'])]:
        source_sha256 = hashlib.sha256((corpus / f'corpus.{suffix}').read_bytes()).hexdigest()
        profiles = {mode: {'compile_ms': []} for mode in ('plain', 'basic')}
        # Alternate order; one serial process at a time under the shared lock.
        for sample in range(5):
            for mode in (('plain', 'basic') if sample % 2 == 0 else ('basic', 'plain')):
                destination = output / f'{suffix}-{mode}.wasm'
                args = [str(binary), kind, '--target', 'wasm32-unknown-unknown', '--emit-wasm']
                for name in names:
                    args += ['--export', name]
                if mode == 'basic':
                    args += ['--wasm-opt', 'basic']
                args += [str(corpus / f'corpus.{suffix}'), '-o', str(destination)]
                start = time.perf_counter_ns()
                subprocess.run(args, check=True, capture_output=True)
                profiles[mode]['compile_ms'].append((time.perf_counter_ns() - start) / 1e6)
        for mode, row in profiles.items():
            destination = output / f'{suffix}-{mode}.wasm'
            row.update(counts(destination.read_bytes()))
            row.update(json.loads(subprocess.check_output(['node', '-e', RUNTIME, str(destination), kind], text=True)))
        assert profiles['plain']['sink'] == profiles['basic']['sink']
        report['corpus'][suffix] = {'source_sha256': source_sha256, 'profiles': profiles}
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
