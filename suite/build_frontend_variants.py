"""Build frozen frontend variants for this pass; restore source in finally.

Run serially before benchmarks. --check-sources validates transformations only.
"""
from pathlib import Path
import os,subprocess,shutil,json,hashlib,argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--base', default='24ca853')
parser.add_argument('--check-sources', action='store_true')
args=parser.parse_args()
root=Path('crates/keel-c/src');out=Path('target/frontend-profile/frontend-opt')
out.mkdir(parents=True,exist_ok=True)
current={p:p.read_text() for p in root.glob('*.rs')}
baseline={}
for p in current:
    result=subprocess.run(['git','show',args.base+':'+str(p)],capture_output=True,text=True)
    if result.returncode==0: baseline[p]=result.stdout
# The first lexer pass is replayed from the original byte-dispatch snapshot.
import tempfile
patch=Path('suite/reports/c-frontend-lexer-base-20261015.patch').read_text()
with tempfile.TemporaryDirectory() as temporary:
    temp=Path(temporary)
    (temp/'lex.rs').write_text(baseline[root/'lex.rs'])
    subprocess.run(['git','apply','--no-index','-'],cwd=temp.resolve(),input=patch,text=True,check=True)
    lexer=(temp/'lex.rs').read_text()
a=current[root/'lex.rs'].index('    fn quoted(');b=current[root/'lex.rs'].index('    /// The escape after',a)
quoted=current[root/'lex.rs'][a:b].replace("Tok<'a>",'Tok').replace('bytes: self.text.bytes(&bytes),','bytes,')
a=lexer.index('    fn quoted(');b=lexer.index('    /// The escape after',a)
lexer=lexer[:a]+quoted+lexer[b:]
a=current[root/'lex.rs'].index('fn parse_number')
b=current[root/'lex.rs'].index('#[cfg(test)]\nmod tests',a)
numbers=current[root/'lex.rs'][a:b].replace("Tok<'a>",'Tok').replace("fn parse_number<'a>", 'fn parse_number')
a=lexer.index('fn parse_number');b=lexer.index('#[cfg(test)]\nmod tests',a)
lexer=lexer[:a]+numbers+lexer[b:]
(out/'lex-rewrite.rs').write_text(lexer)
if args.check_sources:
    print(json.dumps({'baseline':{str(p):hashlib.sha256(t.encode()).hexdigest() for p,t in baseline.items()},'lexer':hashlib.sha256(lexer.encode()).hexdigest(),'arena':{str(p):hashlib.sha256(t.encode()).hexdigest() for p,t in current.items()}},indent=2))
    raise SystemExit(0)
env={**os.environ,'CARGO_PROFILE_RELEASE_DEBUG':'1'}
meta={'command':['cargo','build','--release','-p','keel-c','--features','profile','--example','profile','--example','profile_allocations'],'environment':{'CARGO_PROFILE_RELEASE_DEBUG':'1'},'variants':{}}
def build(name, sources):
 for p,s in sources.items():p.write_text(s)
 subprocess.run(meta['command'],env=env,check=True)
 for src,dst in [('profile','profile-'+name),('profile_allocations','allocations-'+name)]:shutil.copy2('target/release/examples/'+src,out/dst)
 meta['variants'][name]={'sources':{str(p):hashlib.sha256(s.encode()).hexdigest() for p,s in sources.items()},'profiler_sha256':hashlib.sha256((out/('profile-'+name)).read_bytes()).hexdigest(), 'ssa_source_sha256':hashlib.sha256(ssa.read_bytes()).hexdigest()}
 print('Built',name,flush=True)
ssa=Path('crates/keel-frontend/src/lib.rs');ssa_original=ssa.read_text()
try:
 build('baseline',baseline)
 build('lexer',{**baseline,root/'lex.rs':lexer})
 build('arena',current)
 ssa.write_text(ssa_original.replace('    fn find_preds(', '    #[inline(never)]\n    fn find_preds(',1))
 build('ssa-outline',current)
finally:
 for p,s in current.items():p.write_text(s)
 ssa.write_text(ssa_original)
(out/'builds.json').write_text(json.dumps(meta,indent=2)+'\n')
