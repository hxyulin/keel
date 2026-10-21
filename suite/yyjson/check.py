#!/usr/bin/env python3
"""Check every corpus against Python's independent digest, plus strict rejection."""
import json
import pathlib
import subprocess
import sys
import tempfile

from corpus import digest


def check(binary, corpus):
    expected = json.loads((corpus / 'expected.json').read_text())
    for name in ('api', 'config', 'telemetry'):
        path = corpus / f'{name}.json'
        want = digest(json.loads(path.read_bytes()))
        assert want == {k: expected[name][k] for k in want}, name
        result = subprocess.run([binary, str(path), '4'], capture_output=True, text=True)
        assert result.returncode == 0, (name, result.returncode, result.stderr)
        got = json.loads(result.stdout)
        assert got == dict(want, rounds=4), (name, got, want)
        print(json.dumps(dict(case=name, **got), sort_keys=True))
    # Short valid documents catch empty containers, scalar roots, escaping and
    # boundary numbers without relying on the large corpus's shape.
    valid = [b'null', b'true', b'false', b'{}', b'[]', b'""', b'"a\\u0000b"',
             b'["\\ud83d\\ude00",-9223372036854775808,18446744073709551615,-0.0]']
    invalid = [b'{', b'[1,]', b'{"x":1,}', b'01', b'1e', b'true false',
               b'NaN', b'"\\uD800"', b'"\xff"', b'"raw\x00nul"', b'/*x*/null']
    with tempfile.TemporaryDirectory(prefix='yyjson-check-') as tmp:
        path = pathlib.Path(tmp) / 'case.json'
        for raw in valid:
            path.write_bytes(raw)
            result = subprocess.run([binary, str(path), '2'], capture_output=True, text=True)
            assert result.returncode == 0, (raw, result.stderr)
            assert json.loads(result.stdout) == dict(digest(json.loads(raw)), rounds=2)
        for raw in invalid:
            path.write_bytes(raw)
            result = subprocess.run([binary, str(path), '1'], capture_output=True)
            assert result.returncode == 1, (raw, result.returncode, result.stderr)
    print(json.dumps(dict(valid_edges=len(valid), rejected=len(invalid)), sort_keys=True))


if __name__ == '__main__':
    check(str(pathlib.Path(sys.argv[1]).resolve()), pathlib.Path(sys.argv[2]))
