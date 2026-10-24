"""Runner stage/gate regressions with deterministic fake tools; no compilation/timing."""
import json
import pathlib
import subprocess
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch
import run

class RunnerTests(unittest.TestCase):
    def test_ps_failure_defers_even_on_unloaded_host(self):
        for stdout in ['', ' PID %CPU COMM\n']:
            with patch.object(run,'call',return_value=subprocess.CompletedProcess([],1,stdout,'denied')),patch.object(run.os,'getloadavg',return_value=(0,0,0)):
                self.assertFalse(run.quiet()['ok'])
    def test_real_node_rejects_partial_profiles_before_loading_modules(self):
        node=shutil.which('node')
        if not node:self.skipTest('existing Node unavailable')
        with tempfile.TemporaryDirectory(prefix='wasm-review-node-',dir='/tmp') as root:
            manifest=pathlib.Path(root)/'manifest.json'
            for profiles in [{}, {'plain':{'emission':'passed'}}, {m:{'emission':'failed' if m=='clang-O2' else 'passed'} for m in ['plain','basic','clang-O0','clang-O2']}]:
                manifest.write_text(json.dumps({'profiles':profiles}))
                r=subprocess.run([node,str(run.HERE/'harness.cjs'),root,'--correctness-only'],capture_output=True,text=True)
                self.assertNotEqual(r.returncode,0)
                self.assertNotIn('ENOENT',r.stderr)
                self.assertIn('AssertionError',r.stderr)
    def exercise(self, *, static=False, fail=None, gates=None):
        gates=iter(gates or [False,False])
        with tempfile.TemporaryDirectory(prefix='wasm-review-',dir='/tmp') as root:
            root=pathlib.Path(root);binary=root/'input';binary.write_bytes(b'frozen fake binary')
            out=root/'out';calls=[]
            def call(args):
                args=list(map(str,args));calls.append(args);stdout='';rc=0
                if '-p' in args:stdout=sys.executable
                elif args[:2]==['git','rev-parse']:stdout='test-revision'
                elif '--version' in args:stdout='fake version'
                elif '-o' in args:
                    dest=pathlib.Path(args[args.index('-o')+1])
                    if fail and dest.name==fail:rc=1
                    else:dest.write_bytes(b'\0asm\1\0\0\0')
                elif any(a.endswith('harness.cjs') for a in args):
                    report=json.loads((out/'manifest.json').read_text())
                    rc=int(any(p['emission']!='passed' for p in report['profiles'].values()))
                    stdout=json.dumps(dict(correctness_invocations=640)) if not rc else ''
                return subprocess.CompletedProcess(args,rc,stdout,'fake failure' if rc else '')
            def quiet():return dict(ok=next(gates),load=[0,0,0],cpus=2,busy=[])
            argv=['run.py',str(binary),str(out)]+(['--static-only'] if static else [])
            with patch.object(sys,'argv',argv),patch.object(run,'call',side_effect=call),patch.object(run,'quiet',side_effect=quiet),patch.object(run.shutil,'which',return_value=sys.executable),patch.dict(run.os.environ,{'WASM_LD':sys.executable}):
                status=run.main()
            return status,json.loads((out/'manifest.json').read_text()),calls
    def test_busy_gate_keeps_correctness_but_no_timing_samples(self):
        status,r,calls=self.exercise()
        self.assertIsNone(status)
        self.assertEqual(r['correctness']['correctness_invocations'],640)
        self.assertEqual(r['timing'],'deferred: quiet preflight/gate failed')
        self.assertNotIn('compile_ms',r);self.assertNotIn('runtime',r)
        self.assertEqual(sum('-o' in c for c in calls),8)
    def test_static_only_records_intent_without_timing_gate(self):
        status,r,_=self.exercise(static=True,gates=[True])
        self.assertIsNone(status);self.assertEqual(r['timing'],'not requested: static-only')
        self.assertNotIn('timing_gate',r)
    def test_diagnostic_failure_retains_command_and_stage(self):
        status,r,_=self.exercise(fail='clang-O0.o',gates=[True])
        self.assertEqual(status,1);self.assertEqual(r['failure']['stage'],'diagnostic emission')
        self.assertEqual(r['commands'][-1]['returncode'],1)
    def test_failed_profile_cannot_be_a_successful_partial_comparison(self):
        status,r,calls=self.exercise(fail='clang-O2.raw.wasm',gates=[True])
        self.assertEqual(status,1)
        self.assertEqual(r['profiles']['clang-O2']['emission'],'failed')
        self.assertEqual(r['correctness']['status'],'failed')
        self.assertEqual(r['timing'],'not attempted: correctness failed')
        self.assertNotIn('compile_ms',r)
    def test_mid_compile_gate_retains_partial_samples_without_summary(self):
        status,r,_=self.exercise(gates=[True,True,True,False])
        self.assertIsNone(status)
        self.assertEqual(r['timing'],'deferred: per-round quiet gate failed')
        self.assertTrue(all(len(v)==1 for v in r['compile_samples_ms'].values()))
        self.assertNotIn('compile_ms',r);self.assertNotIn('runtime',r)

if __name__=='__main__':unittest.main()
