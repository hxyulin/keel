#!/usr/bin/env python3
"""Independent arbitrary-precision oracle for emitted IR; pass a built keel CLI.

Run manually: python3 crates/keel-opt/tests/constant_division_oracle.py PATH/keel
No dependencies or timing measurements. The Rust tests cover verifier, traps,
locations, pipeline sharing, and native execution separately.
"""

import random
import re
import subprocess
import sys
import tempfile
from pathlib import Path


def evaluate(body, n, width):
    values = {"v0": n}
    mask = (1 << width) - 1
    for line in body.splitlines():
        line = line.strip()
        if line.startswith("return "):
            return values[line.split()[1]]
        match = re.match(r"(v\d+) = (\w+)(?:\.i(\d+))? (.*)", line)
        if not match:
            continue
        result, op, const_width, args = match.groups()
        if op == "iconst":
            values[result] = int(args) & ((1 << int(const_width)) - 1)
            continue
        a, b = (values[v] for v in args.split(", "))
        # Python integers neither overflow nor use host signed interpretation.
        value = {
            "umulhi": lambda: (a * b) >> width,
            "imul": lambda: a * b,
            "iadd": lambda: a + b,
            "isub": lambda: a - b,
            "ushr": lambda: a >> (b % width),
            "and": lambda: a & b,
        }[op]()
        values[result] = value & mask
    raise AssertionError(body)


def main():
    rng = random.Random(0xDEC1A1)
    cases = {}
    source = []
    for width in (32, 64):
        mask = (1 << width) - 1
        divisors = set(range(1, 258))
        divisors.update(1 << b for b in range(width))
        divisors.update(((mask >> 1), (mask >> 1) + 2, mask))
        divisors.update(rng.randrange(1, mask + 1) for _ in range(1000))
        for d in sorted(divisors):
            for op in ("udiv", "urem"):
                name = f"f{len(cases)}"
                cases[name] = (width, d, op)
                immediate = d if d < (1 << 63) else d - (1 << 64)
                source.append(f"func @{name}(i{width}) -> i{width} {{\nblock0(v0: i{width}):\n v1 = iconst.i{width} {immediate}\n v2 = {op} v0, v1\n return v2\n}}")
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "input.keel"
        path.write_text("\n".join(source))
        ir = subprocess.check_output([sys.argv[1], "opt", "--opt-tier", "full", str(path)], text=True)
    checks = 0
    for name, body in re.findall(r"func @(\w+)\([^\n]*\{\n(.*?)\n\}", ir, re.S):
        width, d, op = cases.pop(name)
        mask = (1 << width) - 1
        inputs = {0, 1, d - 1, d, (d + 1) & mask, mask}
        # Exercise discontinuities at low, random, and final quotient multiples.
        for q in (1, 2, mask // d, rng.randrange(mask // d + 1)):
            inputs.update(n for n in (q * d - 1, q * d, q * d + 1) if 0 <= n <= mask)
        inputs.update(rng.randrange(mask + 1) for _ in range(100))
        for n in inputs:
            expected = n // d if op == "udiv" else n % d
            actual = evaluate(body, n, width)
            assert actual == expected, (width, d, op, n, actual, expected, body)
            checks += 1
    assert not cases, f"missing functions: {len(cases)}"
    print(f"PASS: {checks} arbitrary-precision comparisons of full-pipeline IR")


if __name__ == "__main__":
    main()
