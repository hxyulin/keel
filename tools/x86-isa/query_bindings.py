"""Reviewed metadata for existing descriptor/status, selector and scalar I/O APIs.

Enum declarations and ALL membership come from intrinsic-registry.json.
Generated const assertions check the schema's IDs against that registry.
"""
import json
import re

import system_intrinsics

GROUPS = {
    'descriptor_query': ('x86_descriptor_query', "(&'static str, u16, usize)",
                         'Opcode, operand bits, exact write bytes; zero means register.\n'
                         'Long mode only, CPL0 or CR4.UMIP clear; no feature-bit implication.'),
    'selector_query': ('x86_selector_query', "(&'static str, u16, bool)",
                       'Opcode, destination width or zero for verify, exact m16 pointer form.\n'
                       'Pinned even with unused results. Failed LAR/LSL preserve the old destination\n'
                       'at its declared width. Successful wide LAR bits 19:16 are undefined.\n'
                       'Only captured ZF is returned; no general FLAGS preservation is promised.'),
    'port_io': ('x86_port_io', '(bool, u16, bool)',
                'Output direction, data width, encoded immediate port.'),
}


def validate(schema, root):
    if not isinstance(schema, dict) or set(schema) != {'intrinsics'}:
        raise ValueError('unknown or missing query binding schema fields')
    rows = schema['intrinsics']
    if not isinstance(rows, list) or not rows:
        raise ValueError('empty query binding inventory')
    variants, names = set(), set()
    source_names = {r['mnemonic'] for r in json.loads((root / 'zydis.json').read_text())}
    previous = -1
    for row in rows:
        if not isinstance(row, dict) or set(row) != {
                'id', 'variant', 'name', 'params', 'returns', 'pure', 'memory',
                'target', 'doc', 'binding'}:
            raise ValueError('unknown or missing query binding fields')
        if type(row['id']) is not int or row['id'] <= previous:
            raise ValueError('query binding IDs must be nonnegative, unique and ordered')
        previous = row['id']
        for key, seen, pattern in [('variant', variants, r'X86[A-Za-z0-9]+'),
                                    ('name', names, r'x86_[a-z0-9_]+')]:
            if not isinstance(row[key], str) or not re.fullmatch(pattern, row[key]) or row[key] in seen:
                raise ValueError('invalid or duplicate query binding identity')
            seen.add(row[key])
        if row['pure'] is not False or row['memory'] != 'Barrier' or row['target'] != 'x86_64':
            raise ValueError('query bindings require pinned x86_64 barrier policy')
        if not isinstance(row['doc'], str) or not row['doc'].strip():
            raise ValueError('query binding requires a contract description')
        b = row['binding']
        if not isinstance(b, dict) or not isinstance(b.get('kind'), str) or b['kind'] not in GROUPS:
            raise ValueError('unknown query binding kind')
        kind, bits = b['kind'], b.get('bits')
        if type(bits) is not int:
            raise ValueError('invalid query binding width')
        if kind == 'descriptor_query':
            if set(b) != {'kind', 'mnemonic', 'bits', 'write_bytes'}:
                raise ValueError('invalid descriptor binding fields')
            name, write = b['mnemonic'], b['write_bytes']
            if name not in ('sgdt', 'sidt', 'sldt', 'str', 'smsw') or type(write) is not int:
                raise ValueError('invalid descriptor opcode/write footprint')
            legal = (bits, write) == (80, 10) if name in ('sgdt', 'sidt') else (
                (bits, write) == (16, 2) or bits in (16, 32, 64) and write == 0)
            if not legal:
                raise ValueError('invalid descriptor width/write footprint')
            params, returns = (['Ptr'], []) if write else (['I64'], ['I64'])
        elif kind == 'selector_query':
            if set(b) != {'kind', 'mnemonic', 'bits', 'memory'} or type(b['memory']) is not bool:
                raise ValueError('invalid selector binding fields')
            name = b['mnemonic']
            selector = 'Ptr' if b['memory'] else 'I16'
            if name in ('lar', 'lsl') and bits in (16, 32, 64):
                params, returns = ([f'I{bits}', selector], [f'I{bits}', 'I32'])
            elif name in ('verr', 'verw') and bits == 0:
                params, returns = ([selector], ['I32'])
            else:
                raise ValueError('invalid selector opcode/width')
        else:
            if set(b) != {'kind', 'output', 'bits', 'immediate'} or any(
                    type(b[key]) is not bool for key in ('output', 'immediate')) or bits not in (8, 16, 32):
                raise ValueError('invalid scalar port binding fields/width')
            name = 'out' if b['output'] else 'in'
            params = ['I16', f'I{bits}'] if b['output'] else ['I16']
            returns = [] if b['output'] else [f'I{bits}']
        if name not in source_names:
            raise ValueError('query binding opcode absent from pinned import')
        if row['params'] != params or row['returns'] != returns:
            raise ValueError('query binding signature does not match reviewed shape')
    return rows


def generate(schema, root):
    rows = validate(schema, root)
    lines = [system_intrinsics.HEADER,
             '// Reviewed compiler policy in query-bindings.json; not inferred ISA semantics.',
             'use super::{Intrinsic, IntrinsicInfo};', 'use crate::Type;', '',
             'const _: () = {']
    lines += [f'    assert!(Intrinsic::{r["variant"]} as usize == {r["id"]});' for r in rows]
    lines += ['};', '']
    lines += system_intrinsics.metadata(rows, 'query')
    for kind, (method, shape, doc) in GROUPS.items():
        lines += [f'    /// {line}' for line in doc.splitlines()]
        lines += ['    /// See query-bindings.json for contracts.',
                  f'    pub fn {method}(self) -> Option<{shape}> {{', '        Some(match self {']
        for row in rows:
            b = row['binding']
            if b['kind'] != kind:
                continue
            if kind == 'descriptor_query':
                value = f'("{b["mnemonic"]}", {b["bits"]}, {b["write_bytes"]})'
            elif kind == 'selector_query':
                value = f'("{b["mnemonic"]}", {b["bits"]}, {str(b["memory"]).lower()})'
            else:
                value = f'({str(b["output"]).lower()}, {b["bits"]}, {str(b["immediate"]).lower()})'
            lines += [f'            Self::{row["variant"]} => {value},']
        lines += ['            _ => return None,', '        })', '    }', '']
    return '\n'.join(lines + ['}', ''])


def outputs(root):
    schema = json.loads((root / 'query-bindings.json').read_text())
    return {root.parents[1] / 'crates/keel-ir/src/generated/x86_query_intrinsics.rs':
            generate(schema, root)}
