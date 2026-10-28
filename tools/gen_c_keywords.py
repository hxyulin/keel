#!/usr/bin/env python3
"""Regenerate the C parser's keyword association tables using only the stdlib."""

import argparse
from collections import defaultdict
from pathlib import Path
import random
import re


PARSER = Path(__file__).resolve().parents[1] / "crates/keel-c/src/parse.rs"
BEGIN = "// BEGIN GENERATED KEYWORD ASSOCIATIONS"
END = "// END GENERATED KEYWORD ASSOCIATIONS"


def generate(source):
    block = re.search(r"const KEYWORDS:.*?= \[(.*?)\n\];", source, re.S)
    words = re.findall(r'\("([^"]+)",', block.group(1))
    signatures = [
        (len(word), ord(word[0]), ord(word[-1]), ord(word[max(0, len(word) - 3)]))
        for word in words
    ]
    if len(set(signatures)) != len(words):
        raise ValueError("Keyword signatures collide; select another byte position")
    variables = defaultdict(list)
    for index, row in enumerate(signatures):
        for position, char in enumerate(row[1:]):
            variables[position, char].append(index)
    order = sorted(variables, key=lambda key: (-len(variables[key]), key))
    for seed in range(100):
        rng = random.Random(seed)
        values, used = {}, set()
        for variable in order:
            position, _ = variable
            ready = []
            for index in variables[variable]:
                length, *chars = signatures[index]
                others = [(p, c) for p, c in enumerate(chars) if p != position]
                if all(other in values for other in others):
                    ready.append(length + sum(values[other] for other in others))
            candidates = list(range(256))
            if seed:
                rng.shuffle(candidates)
            for value in candidates:
                targets = [(base + value) & 255 for base in ready]
                if len(set(targets)) == len(targets) and not used.intersection(targets):
                    break
            else:
                break
            values[variable] = value
            used.update(targets)
        else:
            assert len(used) == len(words), "Not every keyword received a slot"
            break
    else:
        raise ValueError("No collision-free association found in 100 seeds")

    lines = [BEGIN, "// Regenerate with: python3 tools/gen_c_keywords.py"]
    lines += [
        f"const MIN_KEYWORD_LEN: usize = {min(map(len, words))};",
        f"const MAX_KEYWORD_LEN: usize = {max(map(len, words))};",
    ]
    for position in range(3):
        table = [values.get((position, char), 0) for char in range(256)]
        lines += ["#[rustfmt::skip]", f"const ASSO{position}: [u8; 256] = ["]
        for start in range(0, 256, 16):
            lines.append("    " + ", ".join(map(str, table[start:start + 16])) + ",")
        lines.append("];")
    return "\n".join([*lines, END])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Reject stale tables")
    args = parser.parse_args()
    source = PARSER.read_text()
    updated, count = re.subn(
        re.escape(BEGIN) + r".*?" + re.escape(END),
        lambda _: generate(source), source, flags=re.S,
    )
    if count != 1:
        parser.error("Expected one generated association block in parse.rs")
    if args.check:
        if updated != source:
            parser.error("Keyword associations are stale; rerun without --check")
        print("Keyword associations are current")
    else:
        PARSER.write_text(updated)


if __name__ == "__main__":
    main()
