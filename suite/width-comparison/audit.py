#!/usr/bin/env python3
"""Audit GNU-disassembled v3 objects. Writes evidence to the campaign directory."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys


def instructions(text, symbol):
    body = text.split(f'<{symbol}>:', 1)[1].split('\n\n')[0]
    rows = []
    for line in body.splitlines():
        match = re.match(r'\s*([0-9a-f]+):\s+((?:[0-9a-f]{2} )+)\s*\t(\S+)(.*)', line)
        if match:
            rows.append(dict(address=int(match[1], 16), bytes=len(match[2].split()),
                             mnemonic=match[3], operands=match[4].strip()))
    return rows


def main():
    root = Path(sys.argv[1])
    report = json.loads((root/'results.json').read_text())
    result = dict(base=report['base'], base_manifest_sha256=report['base_manifest_sha256'],
                  widths={}, controls={}, changed_functions=[])
    code = {}
    for w in [128,256]:
        obj = root/f'v3-{w}.o'
        # objcopy avoids parsing grouped hex and preserves exact function bytes.
        object_sha = hashlib.sha256(obj.read_bytes()).hexdigest()
        assert object_sha == report['hashes'][str(obj)], 'object changed since campaign'
        section = root/f'v3-{w}.text'
        subprocess.run(['objcopy', '--dump-section', f'.text={section}', str(obj), str(root/f'v3-{w}-audit-copy.o')], check=True)
        assert hashlib.sha256(obj.read_bytes()).hexdigest() == object_sha, 'audit mutated object'
        symbols = subprocess.check_output(['nm', '-S', '--defined-only', str(obj)], text=True)
        code[w] = {}
        for line in symbols.splitlines():
            fields = line.split()
            if len(fields) == 4 and fields[2] == 'T':
                addr, size = int(fields[0],16), int(fields[1],16)
                code[w][fields[3]] = section.read_bytes()[addr:addr+size]
        rows = instructions((root/f'v3-{w}.dis').read_text(), 'rk_widen32')
        load = next(r for r in rows if r['mnemonic'] in ['pmovsxdq', 'vpmovsxdq'])
        back = next(r for r in rows if r['mnemonic']=='jmp' and
                    r['operands'].split()[0] == f"{load['address']:x}")
        loop = [r for r in rows if load['address'] <= r['address'] <= back['address']]
        loads = [r for r in loop if r['mnemonic'] in ['pmovsxdq', 'vpmovsxdq']]
        assert len(loads) == 4, 'expected four exact widening loads'
        assert all(('QWORD PTR' if w == 128 else 'XMMWORD PTR') in r['operands'] for r in loads)
        assert not any('rsp' in r['operands'] or 'rbp' in r['operands'] for r in loop), 'loop stack access'
        finish_end = next(r for r in rows if r['address'] > back['address'] and r['mnemonic']=='cmp')
        finish = [r for r in rows if back['address'] < r['address'] < finish_end['address']]
        symbol_code = code[w]['rk_widen32']
        result['widths'][w] = dict(function_bytes=len(symbol_code),
            function_sha256=hashlib.sha256(symbol_code).hexdigest(),
            loop_instructions=len(loop), loop_bytes=finish[0]['address']-load['address'],
            loop_vector_copies=sum(r['mnemonic'] in ['movups', 'movaps', 'vmovups', 'vmovaps',
                                                     'movdqa', 'movdqu', 'vmovdqa', 'vmovdqu'] for r in loop),
            loop_widen_load_bytes=8 if w == 128 else 16,
            instructions=rows,
            finish_instructions=len(finish), finish_bytes=finish_end['address']-finish[0]['address'],
            loop=loop, finish=finish)
    assert code[128].keys() == code[256].keys()
    result['changed_functions'] = [k for k in code[128] if code[128][k] != code[256][k]]
    assert result['changed_functions'] == ['rk_widen32']
    for profile in ['v1','v2','avx']:
        for w in [128, 256]:
            rows = instructions((root/f'{profile}-{w}.dis').read_text(), 'rk_widen32')
            newer = [r for r in rows if r['mnemonic'].startswith('v') or 'ymm' in r['operands'] or 'zmm' in r['operands']]
            assert not newer
            result['controls'][f'{profile}-{w}'] = dict(avx_mnemonic_or_wide_operand_matches=len(newer))
    (root/'audit.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({w:{k:v for k,v in x.items() if k not in ['loop','finish','instructions']} for w,x in result['widths'].items()}))


if __name__ == '__main__':
    main()
