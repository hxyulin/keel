#!/usr/bin/env python3
"""Check reduced optimizer/link-readiness regressions; unresolved calls fail the check."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
OPAQUE = 'func @opaque() optnone {\nblock0:\n return\n}\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keel', type=Path, required=True)
    parser.add_argument('--out', type=Path)
    args = parser.parse_args()
    keel = args.keel.resolve()
    cold = (HERE / 'policy-regressions/cold.keel').read_text()
    rounds = (HERE / 'policy-regressions/rounds.keel').read_text()
    dead = (HERE / 'policy-regressions/dead-local.keel').read_text()
    cases = [
        ('cold-control', cold.replace(' cold', ''), True),
        ('cold', cold, False),
        ('explicit-O3', cold.replace(' cold', ' optimize(3)'), False),
        ('explicit-inline', cold.replace(' cold', ' optimize("inline-functions")'), False),
        ('round-order-control', rounds.replace(OPAQUE, ''), True),
        ('round-order', rounds, False),
        ('dead-local-control', dead.replace(OPAQUE, ''), True),
        ('dead-local', dead, False),
    ]
    observations = []
    with tempfile.TemporaryDirectory(prefix='keel-policy-regressions-') as temporary:
        directory = Path(temporary)
        for name, source, control in cases:
            input_path, output_path = directory / (name + '.keel'), directory / (name + '.out.keel')
            input_path.write_text(source)
            result = subprocess.run([str(keel), 'opt', '-O3', str(input_path), '-o', str(output_path)], capture_output=True, text=True)
            output = output_path.read_text() if result.returncode == 0 else ''
            bad_calls = len(re.findall(r'\bcall @absent\(', output))
            observations.append({'case': name, 'control': control, 'returncode': result.returncode,
                                 'diagnostic': result.stderr, 'unresolved_calls': bad_calls,
                                 'remaining_helper_calls': len(re.findall(r'\bcall @helper\(', output)),
                                 'passed': result.returncode == 0 and bad_calls == 0})
    report = {'scope': 'O3 reduced IR link-readiness checks; no benchmark or kernel qualification', 'cases': observations}
    rendered = json.dumps(report, indent=2) + '\n'
    print(rendered, end='')
    if args.out:
        args.out.write_text(rendered)
    raise SystemExit(0 if all(case['passed'] for case in observations) else 1)


if __name__ == '__main__':
    main()
