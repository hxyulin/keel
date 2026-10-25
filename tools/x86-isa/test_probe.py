import unittest

import probe


class ImportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        _, cls.source = probe.load()
        cls.records = probe.records(cls.source)
        cls.report = probe.catalog(cls.source)
        cls.forms = cls.report["forms"]

    def match(self, opcode, modrm=None):
        return [f["mnemonic"] for f in probe.candidates(self.forms, opcode, modrm)]

    def test_repeated_patterns_preserve_long_mode_and_legacy_forms(self):
        lgdt = [r for r in self.records if r["metadata"]["ICLASS"] == "LGDT"]
        self.assertEqual(len(lgdt), 2)
        self.assertIn("mode64", lgdt[0]["pattern"])
        self.assertIn("not64", lgdt[1]["pattern"])
        forms = [f for f in self.forms if f["mnemonic"] == "lgdt"]
        self.assertEqual(len(forms), 1)
        self.assertTrue(forms[0]["force64"])
        self.assertIn("MEM0:r:s64", forms[0]["operands"])
        self.assertEqual(forms[0]["metadata"]["CPL"], "0")

    def test_literal_descriptor_group_witnesses(self):
        for mod, rm in ((0, 0), (1, 4), (2, 5)):
            for reg, mnemonic in ((0, "sgdt"), (1, "sidt"), (2, "lgdt"),
                                  (3, "lidt"), (4, "smsw"), (6, "lmsw"),
                                  (7, "invlpg")):
                with self.subTest(mod=mod, rm=rm, reg=reg):
                    self.assertEqual(self.match([0x0F, 0x01], mod << 6 | reg << 3 | rm), [mnemonic])

    def test_memory_only_forms_reject_register_domain(self):
        for reg in (0, 1, 2, 3):
            self.assertEqual(self.match([0x0F, 0x01], 0xC0 | reg << 3), [])
        self.assertEqual(self.match([0x0F, 0x01]), [])

    def test_swapgs_requires_exact_modrm_and_preserves_other_group_forms(self):
        self.assertEqual(self.match([0x0F, 0x01], 0xF8), ["swapgs"])
        self.assertEqual(self.match([0x0F, 0x01], 0xF9), [])
        self.assertEqual(self.match([0x0F, 0x01], 0x38), ["invlpg"])

    def test_selector_register_and_memory_forms(self):
        for group, name in ((2, "lldt"), (3, "ltr")):
            self.assertEqual(self.match([0x0F, 0x00], group << 3), [name])
            self.assertEqual(self.match([0x0F, 0x00], 0xC0 | group << 3), [name])
        self.assertEqual(self.match([0x0F, 0x01], 0xF0), ["lmsw"])

    def test_fixed_system_opcodes(self):
        for opcode, name in (([0xF4], "hlt"), ([0xFA], "cli"), ([0xFB], "sti"),
                             ([0x0F, 0x06], "clts"), ([0x0F, 0x08], "invd"),
                             ([0x0F, 0x09], "wbinvd"), ([0x0F, 0x30], "wrmsr"),
                             ([0x0F, 0x32], "rdmsr")):
            self.assertEqual(self.match(opcode), [name])
        self.assertEqual(self.match([0x0F, 0xFF]), [])

    def test_unsupported_predicates_are_visible(self):
        self.assertEqual({r["metadata"]["ICLASS"] for r in self.report["unsupported"]},
                         {"MOV_CR", "MOV_DR"})
        self.assertTrue(all("unsupported predicate" in r["reason"]
                            for r in self.report["unsupported"]))
        record = self.records[0].copy()
        record["pattern"] += " NEW_CONSTRAINT()"
        with self.assertRaisesRegex(ValueError, "NEW_CONSTRAINT"):
            probe.normalize(record)

    def test_ambiguous_candidates_are_preserved(self):
        form = next(f for f in self.forms if f["mnemonic"] == "hlt")
        self.assertEqual(len(probe.candidates([form, form], [0xF4])), 2)

    def test_missing_operand_pair_rejected(self):
        with self.assertRaisesRegex(ValueError, "incomplete"):
            probe.records("{\nICLASS: BROKEN\nPATTERN: 0xF4\n}\n")


if __name__ == "__main__":
    unittest.main()
