"""Focused command-selection and evidence-classification checks; no native tools."""
import copy
import struct
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import objinspect
import run as harness


class HarnessTests(unittest.TestCase):
    def test_matrix_and_dialect_commands(self):
        cases = harness.cases()
        self.assertEqual(len(cases), 132)
        self.assertEqual(len({c['id'] for c in cases}), 132)
        with tempfile.TemporaryDirectory() as tmp:
            for case in cases:
                calls = []
                with patch.object(harness, 'run', side_effect=lambda cmd, log: calls.append(cmd) or {'rc': 0, 'stdout': ''}):
                    harness.generate(case, Path(tmp), [])
                compiler = calls[0]
                keel = harness.keel_cmd('keel', case, 'input.s', 'output.o')
                intel_flag = '-masm=intel' if case['lang'] == 'cpp' else 'llvm-args=-x86-asm-syntax=intel'
                self.assertEqual(intel_flag in compiler, case['dialect'] == 'intel')
                if case['target'] == 'x86':
                    self.assertEqual(keel[keel.index('--x86-syntax') + 1], case['dialect'])
                else:
                    self.assertNotIn('--x86-syntax', keel)

    def test_att_error_keeps_real_category(self):
        err = harness.first_error('keel: input.s: line 1: unsupported x86 instruction `bogusq`',
                                  ['bogusq %rax, %rbx'])
        self.assertEqual(err['category'], 'x86 instruction bogusq')
        self.assertEqual(err['source'], 'bogusq %rax, %rbx')

    def result(self, debug=False, target='x86'):
        return dict(generated=True, case=dict(debug=debug, target=target),
                    asm=dict(unchanged=True), source_unchanged=True,
                    ref=dict(rc=0), keel=dict(rc=0), structure=dict(all_equal=True))

    def test_evidence_levels(self):
        r = self.result()
        self.assertEqual(harness.verdict(r), 'structure-equal')
        r['structure'] = {}
        self.assertEqual(harness.verdict(r), 'accepted-uncompared')
        r['ref'] = dict(rc=1, fallback='rustc --emit obj')
        self.assertEqual(harness.verdict(r), 'accepted-uncompared')
        del r['ref']['fallback']
        self.assertEqual(harness.verdict(r), 'reference-rejected')
        r['keel']['rc'] = 1
        self.assertEqual(harness.verdict(r), 'keel-rejected')
        r['asm']['unchanged'] = False
        self.assertEqual(harness.verdict(r), 'source-changed')
        r['recompare_hashes'] = {'keel.o': False}
        self.assertEqual(harness.verdict(r), 'artifact-hash-mismatch')

    def test_metadata_and_runtime_are_independent(self):
        r = self.result(debug=True, target='a64')
        r['runtime'] = dict(ref_ok=True, match=True,
                            ref=dict(dsymutil_rc=0, dsym_verify_rc=0),
                            keel=dict(dsymutil_rc=0, dsym_verify_rc=0))
        self.assertEqual(harness.verdict(r), 'structure-equal+runtime+metadata-unchecked')
        r['dwarf'] = dict(dump_equal=True, ref=dict(dump_rc=0, verify_rc=0),
                          keel=dict(dump_rc=0, verify_rc=1))
        self.assertEqual(harness.verdict(r), 'structure-equal+runtime+metadata-failed')
        r['dwarf']['keel']['verify_rc'] = 0
        r['dwarf']['dump_equal'] = False
        r['dwarf']['dump_equal_modulo_lengths'] = True
        self.assertEqual(harness.metadata_status(r), 'differs')
        r['runtime']['match'] = False
        self.assertEqual(harness.verdict(r), 'RUNTIME-MISMATCH')
        r['runtime']['ref_ok'] = False
        self.assertEqual(harness.verdict(r), 'reference-runtime-failed')

    def object(self, fmt='elf'):
        return dict(format=fmt, cputype=62, header=[1], symbols=[], build_version=None, linker_hints=None,
                    sections={'.text': dict(data=b'\x90', size=1, align=1, type=1, attributes=6,
                                             entsize=0, relocations=[])})

    def test_meaningful_structure_differences(self):
        ref = self.object()
        self.assertTrue(objinspect.compare(ref, copy.deepcopy(ref))['all_equal'])
        for key, value in [('align', 16), ('entsize', 8), ('data', b'\xc3'),
                           ('attributes', 7), ('size', 2), ('relocations', [[0, 1, ['symbol', 'other', 0]]])]:
            got = copy.deepcopy(ref)
            got['sections']['.text'][key] = value
            diff = objinspect.compare(ref, got)
            self.assertFalse(diff['all_equal'], key)
            self.assertFalse(diff['all_equal_benign'], key)
            self.assertFalse(diff['code_data_equal'], key)
        for key, value in [('cputype', 183), ('header', [2]), ('format', 'macho'),
                           ('symbols', [['absolute', 0, 0, 0, ['special', 0xfff1, 42], 0]])]:
            got = copy.deepcopy(ref)
            got[key] = value
            self.assertFalse(objinspect.compare(ref, got)['all_equal'], key)

    def test_missing_debug_is_metadata_difference(self):
        ref = self.object()
        ref['sections']['.debug_info'] = copy.deepcopy(ref['sections']['.text'])
        got = copy.deepcopy(ref)
        del got['sections']['.debug_info']
        diff = objinspect.compare(ref, got)
        self.assertFalse(diff['all_equal'])
        self.assertTrue(diff['code_data_equal'])
        self.assertFalse(diff['class_equal']['debug'])

    def test_relocation_pairs_preserve_association(self):
        ref = self.object('macho')
        records = [[0, 1, 0, 3, ['external', 'a']], [0, 0, 0, 3, ['external', 'b']],
                   [0, 1, 0, 3, ['external', 'c']], [0, 0, 0, 3, ['external', 'd']]]
        ref['sections']['.text']['relocations'] = records
        got = copy.deepcopy(ref)
        got['sections']['.text']['relocations'] = records[2:] + records[:2]
        self.assertTrue(objinspect.compare(ref, got)['all_equal'])
        got['sections']['.text']['relocations'] = [records[i] for i in (0, 3, 2, 1)]
        self.assertFalse(objinspect.compare(ref, got)['all_equal'])

    def test_benign_tier_does_not_hide_named_local(self):
        ref = self.object('macho')
        got = copy.deepcopy(ref)
        got['symbols'] = [['ltmp0', 14, 0, ['.text', 0]]]
        self.assertTrue(objinspect.compare(ref, got)['all_equal_benign'])
        got['symbols'][0][0] = 'ltmp_important'
        self.assertFalse(objinspect.compare(ref, got)['all_equal_benign'])

    def elf_with_addrsig(self, reverse=False, value=7, significant='a'):
        names = b'\0.shstrtab\0.strtab\0.symtab\0.llvm_addrsig\0'
        symbols = [('a', 1, value), ('b', 3, 9)]
        if reverse:
            symbols.reverse()
        symtab = bytes(24) + b''.join(struct.pack('<IBBHQQ', name_at, 0x10, 0, 0xfff1, val, 0)
                                     for _, name_at, val in symbols)
        index = next(i for i, sym in enumerate(symbols, 1) if sym[0] == significant)
        chunks = [b'', names, b'\0a\0b\0', symtab, bytes([index])]
        data = bytearray(64)
        data[:6] = b'\x7fELF\x02\x01'
        headers = [bytes(64)]
        for name, typ, link, entsize, chunk in zip(
                ('.shstrtab', '.strtab', '.symtab', '.llvm_addrsig'),
                (3, 3, 2, 0x6fff4c03), (0, 0, 2, 3), (0, 0, 24, 0), chunks[1:]):
            headers.append(struct.pack('<IIQQQQIIQQ', names.index(name.encode()), typ, 0, 0,
                                       len(data), len(chunk), link, 0, 1, entsize))
            data.extend(chunk)
        shoff = len(data)
        data.extend(b''.join(headers))
        struct.pack_into('<HHIQQQIHHHHHH', data, 16, 1, 62, 1, 0, 0, shoff, 0, 64, 0, 0, 64, 5, 1)
        return objinspect.elf(bytes(data))

    def test_addrsig_indexes_normalize_but_symbol_changes_do_not(self):
        ref = self.elf_with_addrsig()
        got = self.elf_with_addrsig(reverse=True)
        diff = objinspect.compare(ref, got)
        self.assertTrue(diff['all_equal'])
        self.assertFalse(diff['sections']['.llvm_addrsig']['raw_bytes'])
        self.assertFalse(objinspect.compare(ref, self.elf_with_addrsig(significant='b'))['all_equal'])
        changed = self.elf_with_addrsig(value=8)
        self.assertFalse(objinspect.compare(ref, changed)['symbols_equal'])

    def test_complete_diagnostics_retained(self):
        payload = 'x' * 25000
        with patch.object(harness.subprocess, 'run', return_value=type('Result', (), dict(
                returncode=1, stdout=payload.encode(), stderr=payload.encode()))()):
            result = harness.run(['unused'], [])
        self.assertEqual(result['stdout'], payload)
        self.assertEqual(result['stderr'], payload)


if __name__ == '__main__':
    unittest.main()
