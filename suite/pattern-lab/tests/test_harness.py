"""Boundary, failure and statistical properties; native tests are correctness only."""
import copy
import itertools
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))
from lab.manifest import LabError, U64, load
from lab.engine import Runner, build, check, digest, invoke, seal, verify_record, hostarch, bench, resolve_tool, tool_fingerprints
from lab.stats import schedule, summarize, common_iterations

# Keep generated evidence in ignored directories; retain failures and fixes.
TEST_ROOT=Path(os.environ.get('PATTERN_LAB_TEST_ROOT',ROOT.parents[1]/'target/pattern-lab-harness-tests'))
TEST_ROOT.mkdir(parents=True,exist_ok=True)

class Inputs(unittest.TestCase):
    def setUp(self):
        self.root=Path(tempfile.mkdtemp(prefix='input-',dir=TEST_ROOT))
        (self.root/'k.c').write_text('/* test */\n')
        self.m={'schema':1,'id':'test','arch':hostarch(),'description':'test','category':'control','variants':[{'id':v,'source':'k.c','symbol':'pattern_kernel'} for v in ('a','b')],'oracle':{'source':'k.c','symbol':'pattern_oracle'},'data_words':512,'mask':255,'seeds':[0,1,U64],'correctness_iterations':[0,1,2,17,257],'work_per_iteration':1,'work_unit':'operation','notes':[]}
    def parse(self,m):
        p=self.root/'input.json'; p.write_text(json.dumps(m)); return load(p,self.root)
    def test_uint64_exact_and_invalid_numbers(self):
        self.assertEqual(self.parse(self.m)['seeds'][-1],18446744073709551615)
        for bad in (U64+1,-1,True,1.0,'1'):
            m=copy.deepcopy(self.m); m['seeds']=[bad]
            with self.subTest(bad=bad), self.assertRaises(LabError): self.parse(m)
    def test_schema_unknown_fields_ids_bounds(self):
        mutations=[lambda m:m.update(schema=2),lambda m:m.update(typo=0),lambda m:m.update(arch='armv8'),lambda m:m['variants'][1].update(id='a'),lambda m:m.update(mask=512),lambda m:m.update(correctness_iterations=[1,2,17,257]),lambda m:m['oracle'].update(symbol='other')]
        for mutate in mutations:
            m=copy.deepcopy(self.m); mutate(m)
            with self.subTest(m=m),self.assertRaises(LabError): self.parse(m)
    def test_escape_absolute_missing_symlink(self):
        external=self.root.parent/'external.c'; external.write_text('test')
        (self.root/'link.c').symlink_to(external)
        for value in ('../external.c',str(external),'missing.c','link.c'):
            m=copy.deepcopy(self.m); m['variants'][0]['source']=value
            with self.subTest(value=value),self.assertRaises(LabError): self.parse(m)
    def test_duplicate_json_key(self):
        p=self.root/'bad.json'; p.write_text('{"schema":1,"schema":1}')
        with self.assertRaises(LabError): load(p,self.root)

class ToolIdentity(unittest.TestCase):
    def test_apple_launcher_resolves_and_seals_delegate(self):
        d=Path(tempfile.mkdtemp(prefix='tool-resolution-',dir=TEST_ROOT))
        launcher=d/'clang-launcher'; actual=d/'installed-clang'; resolver=d/'xcrun'
        for p,body in [(launcher,'raise SystemExit("launcher must not run")'),(actual,'print("clang fake selected compiler")'),(resolver,'import sys; assert sys.argv[1:]==["--no-cache","--find","clang"]; print('+repr(str(actual))+')')]:
            p.write_text('#!'+sys.executable+'\n'+body+'\n'); p.chmod(0o755)
        r=Runner(d/'out')
        with patch('lab.engine.platform.system',return_value='Darwin'), patch('lab.engine.APPLE_LAUNCHERS',{str(launcher):'clang'}), patch('lab.engine.XCRUN',str(resolver)):
            tool=resolve_tool(r,str(launcher))
        self.assertEqual(tool['path'],str(actual))
        self.assertEqual(tool['requested']['path'],str(launcher))
        hashes=tool_fingerprints(tool)
        for p in (launcher,actual,resolver): self.assertEqual(hashes[str(p)],digest(p))
        rec,text=r.run([tool['path'],'--version'])
        self.assertIn('clang fake selected compiler',text)
        actual.write_text(actual.read_text()+'# changed\n')
        self.assertEqual(verify_record(hashes),[{'path':str(actual),'status':'changed'}])
    def test_explicit_selection_is_not_redirected_or_guessed(self):
        d=Path(tempfile.mkdtemp(prefix='tool-explicit-',dir=TEST_ROOT)); selected=d/'custom-clang'
        selected.write_text('#!'+sys.executable+'\nprint("clang custom")\n'); selected.chmod(0o755)
        r=Runner(d/'out')
        with patch('lab.engine.platform.system',return_value='Darwin'):
            tool=resolve_tool(r,str(selected))
        self.assertEqual(tool['path'],str(selected)); self.assertNotIn('resolver',tool)
        self.assertEqual(r.commands,[])

class Protocol(unittest.TestCase):
    def test_balanced_pairs_and_all_permutations(self):
        for ids in (['a','b'],['a','b','c']):
            s=schedule(ids,2)
            self.assertEqual(len(s),2*len(list(itertools.permutations(ids))))
            for p in itertools.permutations(ids): self.assertEqual(s.count(list(p)),2)
            for position in range(len(ids)):
                self.assertEqual(len({sum(row[position]==v for row in s) for v in ids}),1)
    def test_paired_order_effect_preserved(self):
        blocks=[]
        for order in schedule(['a','b'],3):
            # Order/session scale cancels in pairs, preserving a 2x ratio.
            scale=7 if order[0]=='a' else 23
            blocks.append([{'variant':v,'elapsed':scale*(1 if v=='a' else 2),'unit':'ns'} for v in order])
        result=summarize(blocks,['a','b'],10,draws=200)
        self.assertAlmostEqual(result['comparisons']['b']['ratio_to_first'],2)
        for v in result['comparisons']['b']['conditional_95_interval']: self.assertAlmostEqual(v,2)
        with self.assertRaises(LabError): summarize(blocks[:-1],['a','b'],10)
        blocks[0][0]['elapsed']=0
        with self.assertRaises(LabError): summarize(blocks,['a','b'],10)
    def test_one_pilot_per_variant_common_count_and_cap(self):
        samples=[{'iterations':100,'wall_ns':1000},{'iterations':100,'wall_ns':2000}]
        self.assertEqual(common_iterations(samples,100000000,10000000),10000000)
        with self.assertRaises(LabError): common_iterations(samples,100000000,9999999)
        with self.assertRaises(LabError): common_iterations([{'iterations':1,'wall_ns':0}],100,100)
    def test_campaign_uses_one_pilot_fixed_counts_and_no_reruns(self):
        d=Path(tempfile.mkdtemp(prefix='mock-campaign-',dir=TEST_ROOT))
        r=Runner(d/'out')
        evidence={'inputs':{},'artifacts':{},'compiler':{'status':'unused'},'patterns':[{'id':'mock','status':'checked','manifest':{'id':'mock','seeds':[0],'work_per_iteration':4,'work_unit':'add'},'variants':{v:{'binary':v} for v in ('a','b')}}]}
        args=SimpleNamespace(timer='monotonic',target_ms=100,pilot_iterations=10,max_iterations=1000,cycles=2)
        calls=[]
        def sample(r,binary,m,n,seed,timer):
            calls.append((binary,n))
            return {'status':'ok','iterations':n,'seed':seed,'checksum':seed,'oracle':seed,'wall_ns':n*1000000,'elapsed':n*1000000*(1 if binary=='a' else 2),'unit':'ns'}
        with patch('lab.engine.invoke',side_effect=sample): bench(r,evidence,args)
        self.assertEqual(calls[:2],[('a',10),('b',10)])
        self.assertEqual(len(calls),14) # 2 pilots + 4 warmups + 8 samples
        self.assertTrue(all(n==100 for _,n in calls[2:]))
        self.assertEqual(json.loads((r.output/'mock-fixed.json').read_text())['orders'],[['a','b'],['b','a'],['a','b'],['b','a']])
        self.assertEqual(len(json.loads((r.output/'mock-samples.json').read_text())),4)
        self.assertTrue((r.output/'frozen.json').exists())

    def test_subprocess_timeout_and_log_retention(self):
        d=Path(tempfile.mkdtemp(prefix='timeout-',dir=TEST_ROOT)); r=Runner(d/'out',timeout=.05)
        with self.assertRaises(LabError): r.run([sys.executable,'-c','import time; print("before",flush=True); time.sleep(5)'])
        self.assertEqual(r.commands[0]['status'],'timeout')
        self.assertIn('before',Path(r.commands[0]['stdout']).read_text())
        self.assertTrue((r.output/'command-00000.json').exists())
        with self.assertRaises(FileExistsError): Runner(r.output)
    def test_checksum_protocol_rejection(self):
        class Fake:
            def run(self,*args,**kwargs):
                return {'returncode':0},json.dumps({'status':'ok','iterations':1,'seed':0,'checksum':2,'oracle':3})
        with self.assertRaises(LabError): invoke(Fake(),'unused',{'data_words':1,'mask':0},1,0,'monotonic')
    def test_integrity_mutation(self):
        d=Path(tempfile.mkdtemp(prefix='mutation-',dir=TEST_ROOT)); p=d/'source'; p.write_text('one')
        record={str(p):digest(p)}
        self.assertFalse(verify_record(record)); p.write_text('two')
        self.assertEqual(verify_record(record)[0]['status'],'changed'); p.unlink()
        self.assertEqual(verify_record(record)[0]['status'],'missing')

@unittest.skipUnless(shutil.which('clang') and hostarch() in ('aarch64','x86_64'),'native Clang required')
class Native(unittest.TestCase):
    def test_driver_wrap_zero_checksum_failure_and_unsupported_timer(self):
        root=Path(tempfile.mkdtemp(prefix='native-',dir=TEST_ROOT))
        shutil.copyfile(ROOT/'driver.c',root/'driver.c')
        kernel='#include <stdint.h>\nuint64_t pattern_kernel(uint64_t n,uint64_t s,const uint64_t*d,uint64_t m){(void)d;(void)m;while(n--)s+=2;return s;}\n'
        oracle='#include <stdint.h>\nuint64_t pattern_oracle(uint64_t n,uint64_t s,const uint64_t*d,uint64_t m){(void)d;(void)m;return s+n*2;}\n'
        (root/'kernel.c').write_text(kernel); (root/'oracle.c').write_text(oracle)
        m={'schema':1,'id':'synthetic','arch':hostarch(),'description':'synthetic correctness','category':'control','variants':[{'id':v,'source':'kernel.c','symbol':'pattern_kernel'} for v in ('a','b')],'oracle':{'source':'oracle.c','symbol':'pattern_oracle'},'data_words':1,'mask':0,'seeds':[0,1,U64],'correctness_iterations':[0,1,2,17,257],'work_per_iteration':1,'work_unit':'op','notes':[]}
        mp=root/'manifest.json'; mp.write_text(json.dumps(m))
        r=Runner(root/'good'); evidence=build(r,root,[(mp,m)],'clang'); cases=check(r,evidence)
        self.assertEqual(len(cases),30)
        self.assertIn(1,[s['checksum'] for s in cases if s['seed']==U64 and s['iterations']==1])
        binary=evidence['patterns'][0]['variants']['a']['binary']
        for bad in ('-1','18446744073709551616','1.0',''):
            rec,_=r.run([binary,bad,'0','1','0','monotonic'],allow_failure=True); self.assertEqual(rec['returncode'],2)
        rec,_=r.run([binary,'0','0','1','1','monotonic'],allow_failure=True); self.assertEqual(rec['returncode'],2)
        if hostarch()=='aarch64':
            rec,text=r.run([binary,'0','0','1','0','tsc'],allow_failure=True)
            self.assertEqual(rec['returncode'],3); self.assertEqual(json.loads(text)['status'],'unsupported')
        seal(r,evidence)
        self.assertFalse(verify_record(json.loads((r.output/'integrity.json').read_text())))
        # Separate, intentionally wrong oracle: the native driver must fail.
        (root/'oracle.c').write_text(oracle.replace('s+n*2','s+n*2+1'))
        bad=Runner(root/'bad'); be=build(bad,root,[(mp,m)],'clang')
        with self.assertRaises(LabError): check(bad,be)
        self.assertIn('checksum mismatch', ''.join(Path(c['stderr']).read_text() for c in bad.commands))
        seal(bad,be)

if __name__=='__main__': unittest.main()
