"""Deterministic format/policy tests, independent hand-encoded modules."""
import unittest
from decode import decode,stripped

def section(k,p):return bytes([k,len(p)])+p
HEAD=b'\0asm\1\0\0\0'
def module(code):
    return HEAD+section(1,bytes([1,0x60,0,1,0x7f]))+section(3,bytes([1,0]))+section(10,bytes([1,len(code)])+code)

class DecoderTests(unittest.TestCase):
    def test_body_and_immediates_not_counted_as_opcodes(self):
        row=decode(module(bytes([0,0x41,0x80,1,0x0b])))['functions'][0]
        self.assertEqual((row['body_bytes'],row['locals'],row['instructions']),(5,0,2))
        self.assertEqual(row['operators'][0]['immediates'],[128])
    def test_full_width_negative_signed_constants(self):
        for encoded,want in [(bytes.fromhex('41b9f3ddf179'),-1640531527),(bytes.fromhex('417f'),-1),(bytes.fromhex('427f'),-1),(bytes.fromhex('428080808080808080807f'),-9223372036854775808)]:
            row=decode(module(b'\x00'+encoded+b'\x0b'))['functions'][0]
            self.assertEqual(row['operators'][0]['immediates'],[want])
    def test_custom_sections_strip_only_custom_payload(self):
        core=module(bytes([0,0x41,0,0x0b]));custom=section(0,b'\x01x\x99')
        self.assertEqual(stripped(core+custom),core)
        self.assertEqual(stripped(custom.join([HEAD,core[8:]])),core)
    def test_reject_simd_bulk_fp_and_indirect_calls(self):
        for opcode in [0xfd,0xfc,0x43,0x11,0x2a,0x38]:
            with self.subTest(opcode=opcode),self.assertRaises(ValueError):decode(module(bytes([0,opcode,0,0,0x0b])))
    def test_reject_imports_and_shared_or_growing_memory(self):
        for extra in [section(2,b'\x01'),section(5,b'\x01\x00\x01'),section(5,b'\x01\x03\x01\x01')]:
            with self.assertRaises(AssertionError):decode(HEAD+extra)
    def test_fixed_memory_and_integer_only_signatures(self):
        self.assertEqual(decode(HEAD+section(5,b'\x01\x01\x01\x01'))['memory_pages'],[1,1])
        with self.assertRaises(AssertionError):decode(HEAD+section(1,b'\x01\x60\x01\x7d\x00'))
if __name__=='__main__':unittest.main()
