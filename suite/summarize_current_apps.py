"""Resolve retained current-application samples against matching symbol maps.

Reads evidence only. IP windows are illustrative: cycle-event skid and macOS
coalesced PCs prevent assigning all function cost to these instructions.
"""
import argparse
import collections
import json
import pathlib
import re


def normalize(capture, mac, root, app):
    prefix = root / app / pathlib.Path(capture['raw_prefix']).name
    if mac:
        raw = prefix.with_suffix('.sample.txt').read_text()
        slide = int(re.search(r'Load Address:\s+(0x[0-9a-f]+)', raw)[1], 16) - 0x100000000
        maps = []
    else:
        raw = prefix.with_suffix('.perf-events.txt').read_text()
        maps = [(int(a,16), int(b,16), int(c,16), path.strip()) for a,b,c,path in
                re.findall(r'PERF_RECORD_MMAP2[^\n]*\[(0x[0-9a-f]+)\((0x[0-9a-f]+)\) @ (0x[0-9a-f]+)[^\n]*\]: r-xp (.+)', raw)]
        slide = None
    rows = collections.Counter(); images = collections.Counter(); locations = []
    for row in capture['leaf_rows']:
        if mac:
            m = re.match(r'(.+?)\s+\(in (.+?)\)', row['location'])
            symbol, image = m.groups() if m else ('[unknown]', '[unknown]')
            weight = row['samples']
        else:
            symbol = re.sub(r'\+0x[0-9a-f]+$', '', row['location'])
            image, weight = row['image'], row['weight']
        rows[(symbol, image)] += weight; images[image] += weight
        resolved = dict(row)
        if mac:
            off = re.search(r'\+ (\d+)', row['location']); pc = re.search(r'\[(0x[0-9a-f]+)', row['location'])
            if off and pc: resolved['linked_symbol_start'] = int(pc[1],16) - slide - int(off[1])
        else:
            off = re.search(r'\+0x([0-9a-f]+)$', row['location']); pc = int(row['ip'],16)
            for begin, size, file_offset, path in maps:
                if path == image and begin <= pc < begin+size:
                    resolved['linked_symbol_start'] = pc - begin + file_offset - (int(off[1],16) if off else 0)
                    break
        locations.append((symbol, image, resolved))
    total = sum(rows.values()); assert total == capture['denominator']
    variant = capture['variant']
    def application(image):
        return pathlib.Path(image).name in ['lua-' + variant, 'qjs-' + variant, 'sqlite3-' + variant,
                                           'lua-clang_line', 'qjs-clang_line', 'sqlite3-clang_line']
    app_total = sum(v for (s, i), v in rows.items() if application(i))
    result = {'variant': variant, 'round': capture['round'], 'denominator': total,
              'unit': capture['denominator_unit'], 'application_denominator': app_total,
              'application_percent_of_all': 100*app_total/total,
              'unknown_percent_of_all': 100*sum(v for (s, i), v in rows.items() if s in ['[unknown]', '???'] or i in ['[unknown]', '???'])/total,
              'functions': [{'symbol': s, 'image': i, 'weight': v, 'percent_of_all': 100*v/total,
                             'percent_of_application': 100*v/app_total if application(i) and app_total else None}
                            for (s, i), v in rows.most_common()]}
    return result, locations


def disasm_functions(path):
    text = path.read_text(); matches = list(re.finditer(r'^([0-9a-f]+) <([^>]+)>:\n', text, re.M)); out = collections.defaultdict(list)
    for i, match in enumerate(matches):
        name = match[2]
        out[name].append((int(match[1], 16), text[match.end():matches[i+1].start() if i+1 < len(matches) else len(text)]))
    return out


def resolve_function(root, app, variant, name, locations, mac):
    path = root / app / (variant + '.disasm.txt')
    functions = disasm_functions(path); key = '_' + name if mac else name
    if key not in functions:
        return {'symbol': name, 'variant': variant, 'out_of_line_symbol_present': False,
                'note': 'May be inlined, renamed or eliminated; absence alone is not proof of inlining.'}
    instance_weights = collections.Counter()
    for sym, image, row in locations:
        if sym == name and 'linked_symbol_start' in row:
            instance_weights[row['linked_symbol_start']] += row.get('samples', row.get('weight', 0))
    candidates = functions[key]
    start, body = max(candidates, key=lambda pair: instance_weights[pair[0]])
    if len(candidates) > 1:
        assert instance_weights[start], ('unresolved duplicate', name)
    lines = body.splitlines()
    instructions = [l for l in lines if re.match(r'^\s*[0-9a-f]+:', l)]
    address_indices = {int(m[1],16):i for i,l in enumerate(lines) for m in [re.match(r'^\s*([0-9a-f]+):',l)] if m}
    calls = collections.Counter(re.findall(r'\b(?:bl|callq?|jmpq?)\s+[^\n]*<([^>+]+)', body))
    pcs = []
    for symbol, image, row in locations:
        if symbol != name or row.get('linked_symbol_start', start) != start: continue
        if mac:
            m = re.search(r'\+ ([0-9,]+)', row['location'])
            offsets = list(map(int, m[1].strip(',').split(','))) if m else []
        else:
            m = re.search(r'\+0x([0-9a-f]+)$', row['location']); offsets = [int(m[1], 16)] if m else [0]
        for off in offsets[:2]:
            addr = start + off
            index = address_indices.get(addr)
            pcs.append({'offset': off, 'linked_address': hex(addr),
                        'sample_row_percent_of_all': row['percent'],
                        'offset_has_individual_weight': not mac,
                        'instructions': lines[max(0, index-3):index+5] if index is not None else [],
                        'note': 'Coalesced sample row; no per-PC weight' if mac else 'Cycle sampling may skid; window is not causal attribution'})
    unique = {}; 
    for pc in sorted(pcs, key=lambda x: -x['sample_row_percent_of_all']): unique.setdefault(pc['offset'], pc)
    object_path = None
    linkmap = root / app / (variant + '.link-map.txt')
    if linkmap.exists():
        mapping = linkmap.read_text(errors='replace')
        if mac:
            objects = dict(re.findall(r'^\[\s*(\d+)\] (.+)$', mapping, re.M))
            for address, size, number, sym in re.findall(r'^(0x[0-9A-Fa-f]+)\s+(0x[0-9A-Fa-f]+)\s+\[\s*(\d+)\] (.+)$', mapping, re.M):
                if int(address,16) == start and sym == key: object_path = objects[number]; break
        else:
            for address, size, obj in re.findall(r'^ \.text(?:\.[^ ]+)?\s+(0x[0-9a-f]+)\s+(0x[0-9a-f]+)\s+(.+\.o)$', mapping, re.M):
                if int(address,16) <= start < int(address,16)+int(size,16): object_path = obj; break
    source_stem = pathlib.Path(object_path).name.split('.')[0] if object_path else None
    ir_matches = []
    for ir in sorted((root / app).glob('*.keel')):
        if source_stem and ir.stem != source_stem: continue
        data = ir.read_text()
        m = re.search(r'^func [^\n]*@' + re.escape(name) + r'\(', data, re.M)
        if m:
            end = data.find('\nfunc ', m.end()); section = data[m.start():end if end >= 0 else len(data)]
            slots = {n: int(z) for n,z in re.findall(r'(ss\d+) = stack_slot (\d+)', section)}
            uses = collections.Counter(re.findall(r'\bss\d+\b', section))
            ir_matches.append({'file': str(ir), 'line': data[:m.start()].count('\n')+1,
                               'declaration': section.splitlines()[0],
                               'body_lines': len(section.splitlines()),
                               'stack_objects': len(slots),
                               'unreferenced_stack_objects': sum(uses[n] == 1 for n in slots),
                               'unreferenced_stack_bytes': sum(z for n,z in slots.items() if uses[n] == 1),
                               'stack_object_bytes': sum(map(int,re.findall(r'= stack_slot (\d+)', section))),
                               'calls': collections.Counter(re.findall(r'\bcall @([^ (]+)', section))})
    return {'symbol': name, 'variant': variant, 'out_of_line_symbol_present': True,
            'object_from_text_identical_relink_map': object_path, 'source_stem': source_stem, 'linked_start': hex(start), 'symbol_name_instances': [hex(a) for a,b in candidates], 'resolved_instance_weights': {hex(k):v for k,v in instance_weights.items()}, 'selected_instance': 'Most sampled resolved symbol instance; grouped name shares may include other instances.', 'disassembly': str(path), 'disassembly_address_rows': len(instructions),
            'static_call_or_tailjump_targets': dict(calls),
            'stack_access_instruction_count_static': sum(bool(re.search(r'\[(?:sp|x29)[,\]]|\(%(?:rsp|rbp)\)', l)) for l in instructions),
            'prologue': instructions[:24], 'sampled_windows': list(unique.values())[:5], 'matching_keel_ir': ir_matches}


def main():
    p = argparse.ArgumentParser(description=__doc__); p.add_argument('evidence', type=pathlib.Path)
    p.add_argument('--mac', action='store_true'); args = p.parse_args(); root = args.evidence
    raw = json.loads((root / 'profiles.json').read_text()); normalized = {}; locations = collections.defaultdict(list)
    ranking = collections.defaultdict(collections.Counter)
    for workload, captures in raw.items():
        normalized[workload] = []
        app = workload.split('/')[0]
        for cap in captures:
            result, loc = normalize(cap, args.mac, root, app); normalized[workload].append(result)
            locations[(app, cap['variant'])].extend(loc)
            for row in result['functions']:
                if row['percent_of_application'] is not None:
                    ranking[(app, cap['variant'])][row['symbol']] += row['percent_of_all'] / (2 * len([w for w in raw if w.startswith(app + '/')]))
    evidence = {}
    for app in ['lua', 'quickjs', 'sqlite']:
        selected = list(dict.fromkeys([name for variant in ['keel', 'clang'] for name, _ in ranking[(app, variant)].most_common(5)]))
        evidence[app] = {'ranking_method': 'Equal weight to each workload and capture; ranking by self share of all samples, application symbols only.',
                         'top_five': {variant: ranking[(app, variant)].most_common(5) for variant in ['keel', 'clang']},
                         'functions': [resolve_function(root, app, variant, name, locations[(app, variant)], args.mac)
                                       for name in selected for variant in ['keel', 'clang']]}
    (root / 'profile-summary.json').write_text(json.dumps(normalized, indent=2)+'\n')
    (root / 'code-evidence.json').write_text(json.dumps(evidence, indent=2)+'\n')


if __name__ == '__main__': main()
