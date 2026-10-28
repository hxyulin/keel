#!/usr/bin/env python3
"""Generate fixed application-shaped JSON and an independent semantic oracle."""
import hashlib
import json
import pathlib
import struct
import sys


def digest(value):
    data = bytearray()
    nodes = 0
    string_bytes = 0

    def word(n):
        data.extend(struct.pack('<Q', n))

    def string(s):
        nonlocal string_bytes
        b = s.encode('utf-8')
        data.extend(b's')
        word(len(b))
        data.extend(b)
        string_bytes += len(b)

    def visit(v):
        nonlocal nodes
        nodes += 1
        if v is None:
            data.extend(b'n')
        elif isinstance(v, bool):
            data.extend(b't' if v else b'f')
        elif isinstance(v, str):
            string(v)
        elif isinstance(v, int):
            data.extend(b'-' if v < 0 else b'+')
            word(abs(v))
        elif isinstance(v, float):
            data.extend(b'r' + struct.pack('<d', v))
        elif isinstance(v, list):
            data.extend(b'[')
            word(len(v))
            for item in v:
                visit(item)
            data.extend(b']')
        elif isinstance(v, dict):
            data.extend(b'{')
            word(len(v))
            for k, item in v.items():
                string(k)
                visit(item)
            data.extend(b'}')
        else:
            raise TypeError(type(v))

    visit(value)
    h = 14695981039346656037
    for b in data:
        h = ((h ^ b) * 1099511628211) & ((1 << 64) - 1)
    return dict(hash=f'{h:016x}', nodes=nodes, string_bytes=string_bytes)


def generate(out):
    out.mkdir(parents=True, exist_ok=True)
    notes = ['plain ASCII', 'line\nquote"slash\\tab\t', '中文 café 😀', 'nul\0inside', '']
    api = dict(page=7, next=None, total=1024, users=[
        dict(id=9007199254740992 + i, name=f'user-{i:04d}', active=i % 3 != 0,
             profile=dict(note=notes[i % len(notes)], balance=(i - 512) * 0.125,
                          tags=['reader', f'team-{i % 17}'], avatar=None),
             events=[dict(kind='view', at=1700000000 + i * 60 + j, ok=j != 2)
                     for j in range(i % 5)]) for i in range(1024)])
    nested = {'leaf': [None, True, False, {}, [], '終わり']}
    for depth in range(24):
        nested = {'level': depth, 'children': [nested], 'enabled': True}
    config = dict(version=3, services={f'service-{i}': dict(
        endpoint=f'https://host-{i}.example.test/v1?q=hello&lang=中文',
        retries=i % 8, timeout=0.125 * (i + 1), headers={'X-Trace': notes[i % 5]},
        ports=[8000 + i, 9000 + i], enabled=i % 2 == 0) for i in range(160)},
        nested=nested, limits=[-9223372036854775808, 18446744073709551615,
                               0, 9223372036854775807], empty_key={'': 'value'})
    telemetry = dict(source='sensor-cluster', units=['°C', 'm/s', 'Pa'], samples=[
        [1700000000 + i, (i % 200 - 100) * 0.125, (i * 17 % 8192) * 0.0625,
         i % 13 != 0, None if i % 11 == 0 else i % 101] for i in range(4096)],
        calibration=[1e-100, 1e100, 2.2250738585072014e-308, 1.7976931348623157e308,
                     5e-324, -0.0, 0.1, -123456.789])
    expected = {}
    for name, obj in dict(api=api, config=config, telemetry=telemetry).items():
        # Different layouts exercise whitespace and escaped/unescaped Unicode.
        raw = (json.dumps(obj, ensure_ascii=name == 'config', allow_nan=False,
                          indent=2 if name == 'config' else None,
                          separators=None if name == 'config' else (',', ':')) + '\n').encode()
        (out / f'{name}.json').write_bytes(raw)
        expected[name] = dict(digest(json.loads(raw)), bytes=len(raw),
                              sha256=hashlib.sha256(raw).hexdigest())
    (out / 'expected.json').write_text(json.dumps(expected, indent=2) + '\n')
    return expected


if __name__ == '__main__':
    generate(pathlib.Path(sys.argv[1]))
