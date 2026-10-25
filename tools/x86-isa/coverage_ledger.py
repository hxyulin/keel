"""Generate bounded coverage facts, not architectural or test-pass claims."""
import hashlib
import json
from collections import Counter

import cpu_profiles
import feature_fields
import system_intrinsics

INPUTS = ['zydis.json', 'zydis-source.json', 'keel-extensions.json', 'keel-adapters.json',
          'api-identities.json', 'core-identities.json', 'condition-codes.json',
          'system-intrinsics.json', 'registers.json', 'feature-fields.json',
          'feature-policy.json', 'cpu-profiles.json']


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def adapter_bindings(root, adapters):
    api = json.loads((root / 'api-identities.json').read_text())
    core = json.loads((root / 'core-identities.json').read_text())
    conditions = json.loads((root / 'condition-codes.json').read_text())
    bindings, dynamic = {}, []

    def add(id_, name, spellings, source, encoding, selector=None):
        if id_ in bindings:
            raise ValueError('duplicate coverage adapter identity: ' + id_)
        bindings[id_] = dict(id=id_, mnemonic=name, spellings=spellings,
                             source_mnemonic=source, encoding=encoding,
                             selector=selector or {})

    for r in api['baseline']:
        if 'payload' in r:
            dynamic.append(dict(id='Opcode::' + r['variant'], payload=r['payload']))
        else:
            add('Opcode::' + r['variant'], r['mnemonic'],
                r.get('spellings') or [r['mnemonic']], r['mnemonic'], 'any')
    for r in core:
        if 'fields' in r:
            dynamic.append(dict(id='CoreOpcode::' + r['variant'], fields=r['fields']))
        else:
            add('Opcode::Core(CoreOpcode::' + r['variant'] + ')', r['mnemonic'],
                r['spellings'], r['mnemonic'], 'legacy')
    for r in api['x87']:
        add('Opcode::X87(X87Opcode::' + r['variant'] + ')', r['mnemonic'],
            [r['mnemonic'], *r.get('aliases', [])], r['mnemonic'], 'legacy')
    for group, enum, encoding in [('fixed', 'FixedOpcode', 'legacy'),
                                  ('v2', 'V2Opcode', 'legacy'), ('v3', 'V3Opcode', 'vex')]:
        for r in adapters['identities'][group]:
            id_ = f'Opcode::{enum[:-6]}({enum}::{r["variant"]})'
            matches = [a for a in adapters[group] if
                       a.get('variant') == r['variant'] or a.get('opcode') == id_]
            binding_encoding = encoding
            if not matches and group == 'v3':
                # V3 is an API/profile family, not an encoding: it also exposes legacy forms.
                matches = [a for a in adapters['feature_opcodes'] if a['opcode'] == id_]
                binding_encoding = 'any'
            if len(matches) != 1:
                raise ValueError('missing or ambiguous coverage adapter metadata: ' + id_)
            a = matches[0]
            add(id_, r['mnemonic'], [r['mnemonic'], *r.get('aliases', [])],
                a['source_mnemonic'], binding_encoding, a.get('selector'))
    for name in adapters['extensions']:
        add('Extension::' + name, name, [name], name, 'any')
    for r in conditions:
        for stem, variant in [('j', 'Jcc'), ('set', 'Setcc'), ('cmov', 'Cmovcc')]:
            add(f'Opcode::{variant}({r["code"]})', stem + r['name'],
                [stem + s for s in r['aliases']], stem + r['isa_name'], 'legacy')
    return sorted(bindings.values(), key=lambda b: b['id']), dynamic


def adapter_matches(binding, row):
    return (binding['source_mnemonic'] == row['mnemonic']
            and binding['encoding'] in ('any', row.get('encoding', 'legacy'))
            and all(row.get(k) == v for k, v in binding['selector'].items()))


def compiler_matches(binding, row):
    """Candidate relation only; raw prefixes and complete form validity are not proved."""
    r = binding['lowering']
    visible = [o for o in row.get('operands', []) if o.get('visible', True)]
    kind = r['kind']
    if kind == 'system_register':
        family = 'cr' if r['register'].startswith('cr') else 'dr'
        kinds = [o['operand_type'] for o in visible]
        if row['mnemonic'] != 'mov' or len(kinds) != 2:
            return False
        system, gpr = kinds if r['direction'] == 'write' else list(reversed(kinds))
        return system == family and gpr in ('gpr64', 'gpr16_32_64', 'gpr32_32_64')
    if kind == 'unary':
        if row['mnemonic'] != r['mnemonic'] or len(visible) != 1:
            return False
        o = visible[0]
        return (o['operand_type'] == f'gpr{r["bits"]}' if r['operand'] == 'register' else
                o['operand_type'] == 'mem' and o.get('width64') == r['bits'] // 8)
    names = r['mnemonics'] if kind == 'adjacent_fixed' else [
        r['handler'] if kind == 'handwritten' else r['mnemonic']]
    return row['mnemonic'] in names and not visible


def build(root, data, adapters, exclusion_reasons, form_isa):
    inputs = {name: json.loads((root / name).read_text()) for name in INPUTS}
    if data != inputs['zydis.json'] + inputs['keel-extensions.json'] or adapters != inputs['keel-adapters.json']:
        raise ValueError('coverage data differs from recorded source inputs')
    public, dynamic = adapter_bindings(root, adapters)
    system = system_intrinsics.validate(inputs['system-intrinsics.json'])
    policy = feature_fields.closed_policy(inputs['feature-fields.json'], inputs['feature-policy.json'])
    _, profiles = cpu_profiles.resolve(inputs['cpu-profiles.json'], inputs['feature-fields.json'])
    forms, ids = [], set()
    source_count = len(inputs['zydis.json'])
    for index, row in enumerate(data):
        origin = 'zydis' if index < source_count else 'keel'
        id_ = origin + ':' + digest({k: v for k, v in row.items() if k != 'comment'})
        if id_ in ids:
            raise ValueError('duplicate coverage source form')
        ids.add(id_)
        reasons = exclusion_reasons(row)
        forms.append(dict(id=id_, source=origin, source_index=index if origin == 'zydis' else index - source_count,
                          mnemonic=row['mnemonic'], encoding=row.get('encoding', 'legacy'),
                          isa_set=row['meta_info']['isa_set'], policy_key=form_isa(row),
                          category=row['meta_info']['category'], cpl=row.get('cpl'),
                          excluded_reasons=reasons, table_index=None,
                          decoder='excluded' if reasons else 'disabled_by_policy' if row.get('filters', {}).get('feature_ud0_compat') == '1' else 'registered',
                          assembler_candidates=[], compiler_candidates=[], validation='untracked'))
    included = sorted((i for i, row in enumerate(data) if not exclusion_reasons(row)),
                      key=lambda i: data[i]['mnemonic'])
    for table_index, source_index in enumerate(included):
        forms[source_index]['table_index'] = table_index
    for binding in public:
        binding['candidate_forms'] = [forms[i]['id'] for i in included if adapter_matches(binding, data[i])]
        for i in included:
            if adapter_matches(binding, data[i]):
                forms[i]['assembler_candidates'].append(binding['id'])
    compiler = []
    for binding in system:
        matches = [i for i in included if compiler_matches(binding, data[i])]
        if not matches:
            raise ValueError('compiler recipe has no coverage form candidate: ' + binding['name'])
        compiler.append(dict(id=binding['id'], name=binding['name'], variant=binding['variant'],
                             lowering=binding['lowering'], relation='components' if binding['lowering']['kind'] == 'adjacent_fixed' else 'candidates',
                             candidate_forms=[forms[i]['id'] for i in matches], validation='untracked'))
        for i in matches:
            forms[i]['compiler_candidates'].append(binding['id'])
    sets = []
    for key in sorted({f['policy_key'] for f in forms}):
        members = [f for f in forms if f['policy_key'] == key]
        known = key in policy
        requirements = policy.get(key)
        sets.append(dict(key=key, status='unclassified' if not known else 'excluded_policy' if requirements is None else 'declared',
                         requires=requirements, profiles=[] if not known or requirements is None else [name for name, bits in profiles.items() if set(requirements) <= bits],
                         imported=len(members), catalogue=sum(f['table_index'] is not None for f in members),
                         decoder_registered=sum(f['decoder'] == 'registered' for f in members),
                         assembler_candidates=sum(bool(f['assembler_candidates']) for f in members),
                         compiler_candidates=sum(bool(f['compiler_candidates']) for f in members)))
    summary = dict(imported_zydis=source_count, local_extensions=len(data) - source_count,
                   catalogue=len(included), excluded=len(data) - len(included),
                   decoder_registered=sum(f['decoder'] == 'registered' for f in forms),
                   decoder_disabled=sum(f['decoder'] == 'disabled_by_policy' for f in forms),
                   forms_with_assembler_candidates=sum(bool(f['assembler_candidates']) for f in forms),
                   forms_with_compiler_candidates=sum(bool(f['compiler_candidates']) for f in forms),
                   public_bindings=len(public),
                   bindings_without_candidates=sum(not b['candidate_forms'] for b in public),
                   tracked_compiler_bindings=len(compiler),
                   regular_recipes=sum(b['lowering']['kind'] != 'handwritten' for b in compiler),
                   handwritten_hooks=sum(b['lowering']['kind'] == 'handwritten' for b in compiler),
                   unclassified_policy_keys=sum(s['status'] == 'unclassified' for s in sets))
    return dict(schema_version=1,
                scope='Pinned legacy/VEX import plus Keel extensions; structural facts and candidate relations only.',
                provenance=inputs['zydis-source.json'],
                input_sha256={name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in INPUTS},
                not_enumerated=['EVEX', 'MVEX', 'XOP encoding', '3DNow encoding', 'explicit non-64-bit source forms'],
                validation_scope='No test results or runtime campaign verdicts are ingested. Untracked does not mean untested or unsupported.',
                summary=summary, isa_sets=sets, dynamic_identity_families=dynamic,
                assembler_bindings=public, compiler_bindings=compiler, forms=forms)


def markdown(ledger):
    s = ledger['summary']
    lines = ['<!-- Generated by tools/x86-isa/generate.py; do not edit. -->',
             '# x86 structural coverage ledger', '',
             'See [ledger definitions](../../tools/x86-isa/coverage-ledger.md) before interpreting these counts.',
             'Machine-readable form records are in [coverage.json](coverage.json).', '',
             f'Imported source forms: **{s["imported_zydis"]}**, plus **{s["local_extensions"]}** local extension.',
             f'Production catalogue: **{s["catalogue"]}** forms; **{s["excluded"]}** imported forms excluded.',
             f'Decoder registration enabled: **{s["decoder_registered"]}**; disabled by policy: **{s["decoder_disabled"]}**.',
             f'Forms with declarative assembler candidates: **{s["forms_with_assembler_candidates"]}**.',
             f'Public binding records: **{s["public_bindings"]}**; without a direct source join: **{s["bindings_without_candidates"]}** (not an unsupported count).',
             f'Tracked compiler declarations: **{s["tracked_compiler_bindings"]}**, with **{s["regular_recipes"]}** regular recipes and **{s["handwritten_hooks"]}** hooks.',
             f'ISA policy keys without an explicit feature classification: **{s["unclassified_policy_keys"]}**.', '',
             '**Registration is not successful decoding. Candidate links are not complete operand support.**',
             'Compiler coverage outside the tracked system declarations is unknown here.',
             'Per-form validation is untracked; existing tests and campaign records are not inferred from file names.', '',
             '## Excluded source records', '', '| Reason | Records |', '|---|---:|']
    counts = Counter(reason for f in ledger['forms'] for reason in f['excluded_reasons'])
    lines += [f'| `{reason}` | {count} |' for reason, count in sorted(counts.items())]
    lines += ['', 'Reasons can overlap. These counts do not enumerate encodings outside the import.', '',
              '## ISA policy and structural links', '',
              'Profile columns describe declared feature containment, not privilege, OS state or instruction completeness.',
              'An unclassified key is not automatically baseline; the runtime feature lookup currently defaults missing keys to NONE.', '',
              '| ISA policy key | Imported | Catalogue | Decoder registered | Assembler candidates | Compiler candidates | Feature policy | Profiles |',
              '|---|---:|---:|---:|---:|---:|---|---|']
    for row in ledger['isa_sets']:
        policy = ', '.join(row['requires']) or 'baseline' if row['status'] == 'declared' else row['status']
        lines.append(f'| `{row["key"]}` | {row["imported"]} | {row["catalogue"]} | {row["decoder_registered"]} | {row["assembler_candidates"]} | {row["compiler_candidates"]} | {policy} | {", ".join(row["profiles"])} |')
    lines += ['', '## Tracked compiler bindings', '',
              '| ID | Intrinsic | Recipe | Form candidates/components |', '|---:|---|---|---:|']
    lines += [f'| {b["id"]} | `{b["name"]}` | `{b["lowering"]["kind"]}` | {len(b["candidate_forms"])} |' for b in ledger['compiler_bindings']]
    return '\n'.join(lines) + '\n'


def json_report(ledger):
    # One object per record keeps changes to this generated inventory reviewable.
    lines = ['{']
    for index, (key, value) in enumerate(ledger.items()):
        if key in ('forms', 'assembler_bindings', 'compiler_bindings', 'isa_sets'):
            lines.append('  ' + json.dumps(key) + ': [')
            lines += ['    ' + json.dumps(row) + (',' if i + 1 < len(value) else '')
                      for i, row in enumerate(value)]
            lines.append('  ]')
        else:
            lines += json.dumps({key: value}, indent=2).splitlines()[1:-1]
        if index + 1 < len(ledger):
            lines[-1] += ','
    return '\n'.join(lines + ['}', ''])


def outputs(root, data, adapters, exclusion_reasons, form_isa):
    ledger = build(root, data, adapters, exclusion_reasons, form_isa)
    target = root.parents[1] / 'docs/x86-coverage'
    return {target / 'coverage.json': json_report(ledger),
            target / 'README.md': markdown(ledger)}
