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
    def test_reject_cross_body_immediates_and_local_groups(self):
        # A malformed first body must not consume the next body's size byte.
        for first in [bytes([0,0x41,0x80]),bytes([1,1])]:
            payload=bytes([2,len(first)])+first+bytes([2,0,0x0b])
            b=HEAD+section(1,b'\x01\x60\x00\x00')+section(3,b'\x02\x00\x00')+section(10,payload)
            with self.assertRaises(ValueError):decode(b)
    def test_reject_oversized_or_out_of_range_constants(self):
        for encoded in ['418080808010','41808080808000','4280808080808080808001']:
            with self.assertRaises(ValueError):decode(module(b'\x00'+bytes.fromhex(encoded)+b'\x0b'))
    def test_reject_noninteger_or_nonmvp_block_types(self):
        for blocktype in [0x7d,0x7c,0x00]:
            with self.assertRaises(ValueError):decode(module(bytes([0,2,blocktype,11,11,65,0,11])))
    def test_reject_truncated_duplicate_and_trailing_sections(self):
        for b in [HEAD+b'\x00\x02x',HEAD+section(1,b'\x00')*2,HEAD+section(1,b'\x00x'),HEAD+section(3,b'\x01\x00')]:
            with self.assertRaises((ValueError,AssertionError)):decode(b)
    def test_integer_global_policy(self):
        decode(HEAD+section(6,bytes.fromhex('017f01417f0b')))
        for payload in ['017d0043000000000b','017e0041000b']:
            with self.assertRaises(AssertionError):decode(HEAD+section(6,bytes.fromhex(payload)))
    def test_reject_multivalue_signature(self):
        with self.assertRaises(AssertionError):decode(HEAD+section(1,bytes.fromhex('016000027f7f')))
if __name__=='__main__':unittest.main()
