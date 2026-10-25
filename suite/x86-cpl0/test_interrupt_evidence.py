import unittest
from unittest import mock

import interrupt_evidence as evidence
import run


def result(status='pass', code=33, text=None):
    if text is None:
        text = (run.READY.decode() + 'KEEL IRQ HLT 0000000000100001\n'
                + 'KEEL IRQ STIHLT 0000000000100012\n' + run.PASS.decode())
    return dict(status=status, returncode=code, transcript=text)


class InterruptEvidenceTests(unittest.TestCase):
    def test_report_requires_exact_layout_and_normal_success(self):
        good = result()
        self.assertEqual(evidence.report(good), (0x100001, 0x100012))
        for value in [result(status='timeout'), result(code=35),
                      result(text=good['transcript'] + 'extra'),
                      result(text=good['transcript'].replace('STIHLT', 'HLT')),
                      result(text=good['transcript'].replace('0000000000100001', '100001'))]:
            self.assertIsNone(evidence.report(value))

    def test_hlt_boundary_and_pair_adjacency_are_independent(self):
        listing = '  100010:\tfb\tsti\n  100011:\tf4\thlt\n  100012:\tc3\tret\n'
        self.assertEqual(evidence.halt_resume(listing, True), 0x100012)
        gap = '  100010:\tfb\tsti\n  100011:\t90\tnop\n  100012:\tf4\thlt\n'
        self.assertIsNone(evidence.halt_resume(gap, True))
        self.assertEqual(evidence.halt_resume(gap), 0x100013)
        for bad in ['', listing + '  100013:\tf4\thlt\n', '  100011:\tf3 f4\thlt\n']:
            self.assertIsNone(evidence.halt_resume(bad))

    def test_observation_binds_both_reported_addresses(self):
        raw = mock.Mock(returncode=0, stdout=' 100000:\tf4\thlt\n')
        pair = mock.Mock(returncode=0, stdout=' 100010:\tfb\tsti\n 100011:\tf4\thlt\n')
        with mock.patch.object(evidence.subprocess, 'run', side_effect=[raw, pair]):
            proof = evidence.observe(result(), 'guest.elf', 'objdump')
        self.assertTrue(proof['valid'])
        with mock.patch.object(evidence.subprocess, 'run', side_effect=[raw, pair]):
            wrong = evidence.observe(result(text=result()['transcript'].replace('100001', '100002')), 'guest.elf', 'objdump')
        self.assertFalse(wrong['valid'])

    def test_negative_controls_require_normal_failure_or_rejected_report(self):
        valid = dict(report=True, valid=True)
        rejected = dict(report=True, valid=False)
        absent = dict(report=False, valid=False)
        self.assertTrue(evidence.expected_verdict(result(), valid, False))
        self.assertFalse(evidence.expected_verdict(result(), valid, True))
        self.assertTrue(evidence.expected_verdict(result(), rejected, True))
        self.assertTrue(evidence.expected_verdict(result('guest-error', 35, (run.READY + run.FAIL).decode()), absent, True))
        for bad in [result('timeout', 35), result('guest-error', -9), result(text='bad')]:
            for proof in [absent, rejected, valid]:
                self.assertFalse(evidence.expected_verdict(bad, proof, True))
                self.assertFalse(evidence.expected_verdict(bad, proof, False))

    def test_objdump_errors_cannot_be_negative_evidence(self):
        with mock.patch.object(evidence.subprocess, 'run', return_value=mock.Mock(returncode=1, stderr='error')):
            with self.assertRaises(run.HarnessError):
                evidence.observe(result(), 'guest.elf', 'objdump')


if __name__ == '__main__':
    unittest.main()
