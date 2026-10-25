#!/usr/bin/env python3
"""Read-only structural inventory for docs/x86-system-audit.md.

The roster is an audit boundary, not a machine-derived baseline definition.
Candidate links are not assembler, compiler or execution support verdicts.
"""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GROUPS = {
    'descriptor_tables': 'sgdt sidt lgdt lidt',
    'selector_status': 'sldt str smsw',
    'descriptor_queries': 'lar lsl verr verw',
    'system_mutation': 'lldt ltr lmsw invlpg',
    'cpu_cache': 'clts invd wbinvd',
    'msr': 'rdmsr wrmsr',
    'interrupt_control': 'cli sti hlt',
    'gs_exchange': 'swapgs',
    'interrupt_returns': 'iret iretd iretq',
    'fast_entry_return': 'syscall sysret sysenter sysexit',
    'software_interrupts': 'int int1 int3',
    'segment_pointer_loads': 'lfs lgs lss',
    'scalar_port_io': 'in out',
    'string_port_io': 'insb insw insd outsb outsw outsd',
    'smm': 'rsm',
    'cpu_queries': 'cpuid rdtsc rdpmc',
    'flags_stack': 'pushf pushfq popf popfq',
    'fp_context': 'fxsave fxsave64 fxrstor fxrstor64',
}
SWEEP_CATEGORIES = {'SYSTEM', 'SYSRET', 'SYSCALL', 'INTERRUPT', 'SEGOP',
                    'IO', 'IOSTRING'}


def inventory():
    ledger_path = ROOT / 'docs/x86-coverage/coverage.json'
    ledger_bytes = ledger_path.read_bytes()
    ledger = json.loads(ledger_bytes)
    sources = {
        'zydis': json.loads((ROOT / 'tools/x86-isa/zydis.json').read_text()),
        'keel': json.loads((ROOT / 'tools/x86-isa/keel-extensions.json').read_text()),
    }
    # Refuse stale joins. Other recorded policy hashes must match as well.
    for name, digest in ledger['input_sha256'].items():
        path = ROOT / 'tools/x86-isa' / name
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError(f'stale ledger input: {name}; run generate.py')

    groups = {key: [] for key in GROUPS}
    groups.update({key: [] for key in (
        'control_debug_mov', 'segment_mov', 'far_call_jump', 'far_return',
        'segment_push_pop')})
    sweep = {}
    for form in ledger['forms']:
        raw = sources[form['source']][form['source_index']]
        name = form['mnemonic']
        operands = [o for o in raw.get('operands', []) if o.get('visible', True)]
        kinds = {o['operand_type'] for o in operands}
        keys = [key for key, names in GROUPS.items() if name in names.split()]
        if name == 'mov' and kinds & {'cr', 'dr'}:
            keys.append('control_debug_mov')
        if name == 'mov' and 'sreg' in kinds:
            keys.append('segment_mov')
        if 'far_branch' in raw.get('flags', []):
            if name in ('call', 'jmp'):
                keys.append('far_call_jump')
            elif name == 'ret':
                keys.append('far_return')
        if name in ('push', 'pop') and any(
                o.get('register') in ('fs', 'gs') for o in operands):
            keys.append('segment_push_pop')
        for key in keys:
            groups[key].append(form)
        if form['category'] in SWEEP_CATEGORIES or form['cpl'] == 0:
            sweep.setdefault(form['isa_set'], []).append(form)

    def describe(forms):
        return {
            'imported': len(forms),
            'catalogue': sum(f['table_index'] is not None for f in forms),
            'mnemonics': sorted({f['mnemonic'] for f in forms}),
            'forms': forms,
        }

    return {
        'ledger_sha256': hashlib.sha256(ledger_bytes).hexdigest(),
        'scope': 'Manual system roster plus category/CPL sweep; no support verdicts.',
        'groups': {key: describe(forms) for key, forms in groups.items()},
        'category_cpl_sweep': {key: describe(forms)
                               for key, forms in sorted(sweep.items())},
    }


if __name__ == '__main__':
    print(json.dumps(inventory(), indent=2))
