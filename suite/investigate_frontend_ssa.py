"""Diagnostic SSA counters only; restore source in finally.

Rebuild the normal profiling example afterwards; use no diagnostic timings.
"""
from pathlib import Path
import subprocess,os,shutil,json,hashlib
ssa=Path('crates/keel-frontend/src/lib.rs');original=ssa.read_text()
s=original.replace("pub struct FunctionBuilder<'f> {", "pub struct FunctionBuilder<'f> {\n    pred_queries: std::cell::Cell<usize>,\n    pred_visits: std::cell::Cell<usize>,",1)
s=s.replace('''        Self {
            func,''','''        Self {
            pred_queries: std::cell::Cell::new(0),
            pred_visits: std::cell::Cell::new(0),
            func,''',1)
s=s.replace('''            if !changed {
                return;''','''            if !changed {
                eprintln!("SSA_DIAG {{\\\"name\\\":{:?},\\\"blocks\\\":{},\\\"queries\\\":{},\\\"block_visits\\\":{}}}", self.func.name, self.func.blocks().count(), self.pred_queries.get(), self.pred_visits.get());
                return;''',1)
s=s.replace('''    fn find_preds(&self, block: Block) -> Vec<(Inst, usize)> {
        let mut preds = Vec::new();
        for b in self.func.blocks() {''','''    fn find_preds(&self, block: Block) -> Vec<(Inst, usize)> {
        self.pred_queries.set(self.pred_queries.get() + 1);
        let mut preds = Vec::new();
        for b in self.func.blocks() {
            self.pred_visits.set(self.pred_visits.get() + 1);''',1)
assert 'SSA_DIAG' in s and 'pred_visits.set' in s
out=Path('target/frontend-profile/frontend-opt')
try:
 ssa.write_text(s)
 subprocess.run(['cargo','build','--release','-p','keel-c','--features','profile','--example','profile'],env={**os.environ,'CARGO_PROFILE_RELEASE_DEBUG':'1'},check=True)
 shutil.copy2('target/release/examples/profile',out/'profile-ssa-counts')
finally:ssa.write_text(original)
source=json.loads(Path('suite/reports/c-frontend-stages-20261015.json').read_text())
report={'method':'Diagnostic-only FunctionBuilder counters count find_preds queries and built-block visits. No diagnostic timings are used as latency. Each normal input is compiled three times by profile baseline 1: public warmup, staged warmup, measured public API. Aggregate counts divide all diagnostic records by three. Final production source has no counters.','original_source_sha256':hashlib.sha256(original.encode()).hexdigest(),'diagnostic_source_sha256':hashlib.sha256(s.encode()).hexdigest(),'diagnostic_patch':__import__('difflib').unified_diff(original.splitlines(True),s.splitlines(True),fromfile='production',tofile='diagnostic'),'applications':{}}
report['diagnostic_patch']=''.join(report['diagnostic_patch'])
for app,inputs in source['inputs'].items():
 command=[str((out/'profile-ssa-counts').resolve()),'baseline','1',*[r['path'] for r in inputs]]
 p=subprocess.run(command,capture_output=True,text=True,check=True)
 rows=[json.loads(line[len('SSA_DIAG '):]) for line in p.stderr.splitlines() if line.startswith('SSA_DIAG ')]
 assert len(rows)%3==0
 totals={k:sum(r[k] for r in rows)//3 for k in ['blocks','queries','block_visits']}
 assert totals['queries']==totals['blocks']
 unique={tuple(r[k] for k in ['name','blocks','queries','block_visits']):r for r in rows}
 report['applications'][app]={'command':command,'functions':len(rows)//3,'totals':totals,'largest_scans':sorted(unique.values(),key=lambda r:r['block_visits'],reverse=True)[:12]}
 print(app,report['applications'][app],flush=True)
Path('suite/reports/c-frontend-ssa-investigation-20261015.json').write_text(json.dumps(report,indent=2)+'\n')
