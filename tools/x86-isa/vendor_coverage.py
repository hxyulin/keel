"""Compare public C wrapper names against the pinned SSE–AVX512F/VL/DQ/BW/CD reference headers.

This is a declaration inventory, not a semantic or feature conformance claim.
Only function definitions and preprocessor definitions count. References inside
comments, wrapper bodies, and documentation do not add names. Legacy MMX entries
are reported separately because they require a different register-state contract.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

HEADERS = (
    'xmmintrin.h', 'emmintrin.h', 'pmmintrin.h', 'tmmintrin.h',
    'smmintrin.h', 'avxintrin.h', 'avx2intrin.h', 'crc32intrin.h',
    'avx512fintrin.h', 'avx512vlintrin.h',
    'avx512cdintrin.h', 'avx512vlcdintrin.h', 'avx512dqintrin.h', 'avx512vldqintrin.h', 'avx512bwintrin.h', 'avx512vlbwintrin.h',
)
TOKEN = re.compile(r'/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|[A-Za-z_]\w*|[^\s]', re.S)
PUBLIC = re.compile(r'(?:_mm(?:256|512)?_|_k)\w+\Z')


def definitions(source: str) -> dict[str, bool]:
    """Return public names and whether their definition mentions MMX values."""
    # Keep line boundaries for macro directives; remove comments before matching.
    uncommented = TOKEN.sub(lambda match: '\n' * match[0].count('\n')
                            if match[0].startswith(('/*', '//')) else match[0], source)
    logical = uncommented.replace('\\\n', '')
    result = {name: '__m64' in body or bool(re.search(r'_pi(?:8|16|32)\b', name))
              for name, body in re.findall(r'^\s*#\s*define\s+((?:_mm(?:256|512)?_|_k)\w+)\b([^\n]*)', logical, re.M)}
    tokens = [token for token in TOKEN.findall(source)
              if not token.startswith(('/*', '//'))]
    boundary = 0
    depth = 0
    i = 0
    while i < len(tokens):
        token = tokens[i]
        if token == '{':
            depth += 1
        elif token == '}':
            depth -= 1
            if depth == 0:
                boundary = i + 1
        elif token == ';' and depth == 0:
            boundary = i + 1
        if depth == 0 and PUBLIC.fullmatch(token) and i + 1 < len(tokens) and tokens[i + 1] == '(':
            j, parens = i + 2, 1
            while j < len(tokens) and parens:
                parens += (tokens[j] == '(') - (tokens[j] == ')')
                j += 1
            if j < len(tokens) and tokens[j] == '{':
                result[token] = '__m64' in tokens[boundary:j]
        i += 1
    return result


def inventory(reference: Path, owned: Path) -> dict:
    """Inventory the reference boundary against all compiler-owned fragments."""
    local = set()
    for header in sorted(owned.glob('*.h')):
        local.update(definitions(header.read_text()))
    headers = {}
    for header in HEADERS:
        names = definitions((reference / header).read_text())
        mmx = {name for name, legacy in names.items() if legacy or name == '_mm_empty'}
        candidates = set(names) - mmx
        headers[header] = {
            'present': sorted(candidates & local),
            'missing': sorted(candidates - local),
            'excluded_mmx': sorted(mmx),
        }
    return {
        'contract': 'Public wrapper names only; presence does not establish semantic, signature, immediate, feature, or ABI compatibility.',
        'exclusions': ['MMX values and state', 'AVX-512 extensions beyond AVX512F/VL/DQ/BW/CD', 'AMX', 'C++ overloads', '32-bit x86', 'unlisted headers'],
        'headers': headers,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference-dir', type=Path, required=True)
    parser.add_argument('--owned-dir', type=Path, default=Path(__file__).resolve().parents[2] / 'crates/keel-c/include')
    args = parser.parse_args()
    print(json.dumps(inventory(args.reference_dir, args.owned_dir), indent=2))


if __name__ == '__main__':
    main()
