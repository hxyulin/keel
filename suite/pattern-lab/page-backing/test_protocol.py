import collections
import importlib.util
import os
import pathlib
import struct
import unittest

path=pathlib.Path(__file__).with_name('run.py')
spec=importlib.util.spec_from_file_location('page_backing_runner',path)
runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)

class Protocol(unittest.TestCase):
    def test_physical_sets_are_disjoint_and_orders_share_frames(self):
        pfns=list(range(4096,8192))[::-1]
        result=runner.layouts(pfns,317,8,20261018)
        sets=[]
        for group in range(8):
            ordered,shuffled=result[group*2:group*2+2]
            self.assertEqual(ordered['pfns'],sorted(ordered['pfns']))
            self.assertEqual(set(ordered['pfns']),set(shuffled['pfns']))
            self.assertNotEqual(ordered['pfns'],shuffled['pfns'])
            self.assertEqual([pfns[i] for i in shuffled['indices']],shuffled['pfns'])
            sets.append(set(ordered['pfns']))
        self.assertEqual(len(set.union(*sets)),317*8)

    def test_measured_orders_balance_each_variant_position_and_permutation(self):
        schedule=runner.schedule(2,16,['A','B','N'],6,12345)
        self.assertEqual(len(schedule),8)
        for workload in range(2):
            for placement in range(16):
                orders=[tuple(arm for w,p,arm in block if (w,p)==(workload,placement)) for block in schedule[2:]]
                self.assertEqual(len(set(orders)),6)
                for position in range(3):
                    self.assertEqual(collections.Counter(x[position] for x in orders),dict(A=2,B=2,N=2))

    def test_invalid_geometry_and_block_counts_rejected(self):
        with self.assertRaises(ValueError):runner.layouts(list(range(100)),30,4,1)
        with self.assertRaises(ValueError):runner.layouts([1]*100,10,4,1)
        with self.assertRaises(ValueError):runner.schedule(1,1,['A','B','N'],7,1)

    def test_elf_own_variant_extent_and_rejections(self):
        data=bytearray(4096*3)
        struct.pack_into('<16sHHIQQQIHHHHHH',data,0,b'\x7fELF\x02\x01'+b'\0'*10,3,62,1,0,64,0,0,64,56,1,0,0,0)
        struct.pack_into('<IIQQQQQQ',data,64,1,5,4096,4096,0,4096,4096,4096)
        data[4096:8192]=b'X'*4096
        actual=runner.elf_code(data)
        self.assertEqual(actual,dict(offset=4096,vaddr=4096,length=4096,code=b'X'*4096))
        struct.pack_into('<I',data,68,7)
        with self.assertRaises(ValueError):runner.elf_code(data)
        with self.assertRaises(ValueError):runner.elf_code(b'short')

    def test_ready_barrier_requires_complete_acknowledgement(self):
        r,w=os.pipe()
        try:
            expected=(runner.MAGIC,0x1000,4096,1)
            os.write(w,struct.pack('=4Q',*expected))
            self.assertEqual(runner.read_ready(r,1),expected)
        finally:os.close(r);os.close(w)
        r,w=os.pipe();os.close(w)
        try:
            with self.assertRaises(ValueError):runner.read_ready(r,1)
        finally:os.close(r)

if __name__=='__main__':unittest.main()
