#!/usr/bin/env python3
"""Exported native-only proof: exact roster, hashes, both toolchains, no skips."""
import hashlib,json,pathlib,platform,subprocess,sys,os,struct,shutil,re
p=pathlib.Path(__file__).resolve().parent
if sys.argv[1:] in ([],['--check-compilers']): (p/'results.json').unlink(missing_ok=True)
if not __debug__: raise SystemExit('optimized Python disables acceptance checks')
plans=[a+'-'+pool for a in ['Fast','Full'] for pool in ['normal','tiny','spill']]+['shared-first','shared-second']
names=[f'{cpu}-{s}-{plan}-{opt}-{kind}' for cpu in ['v1','v2','v3'] for s in ['Baseline','Trees','Full'] for plan in plans for opt in ['raw','opt'] for kind in ['typed','text']]
assert len(names)==288 and len(set(names))==288
assert (p/'objects.txt').read_text().splitlines()==names
files=['README.md','encoding.S','probe.S','test.c','test.ir','run.py','objects.txt']+[n+'.o' for n in names]+[n.removesuffix('-typed')+'.s' for n in names if n.endswith('-typed')]
allowed_objects={n+'.o' for n in names}|{'encoding.S.o','probe.S.o','test.c.o','encoding.o'}
assert {f.name for f in p.glob('*.o')}<=allowed_objects,'unlisted object in packet'
def hashes(): return {f:hashlib.sha256((p/f).read_bytes()).hexdigest() for f in files}
if sys.argv[1:]==['--seal']:
 (p/'manifest.json').write_text(json.dumps(hashes(),indent=2)+'\n');sys.exit(0)
assert json.loads((p/'manifest.json').read_text())==hashes(),'packet hash mismatch'
for n in names:
 b=(p/(n+'.o')).read_bytes();assert b[:7]==b'\x7fELF\x02\x01\x01' and b[16:20]==b'\x01\x00\x3e\x00','invalid object'
if sys.argv[1:]==['--check']:print('PASS sealed x87-numerical packet 288 objects');sys.exit(0)
def compiler_identity(command,implementation):
 driver=shutil.which(command)
 assert driver is not None,f'missing compiler driver: {command}'
 driver=str(pathlib.Path(driver).absolute())
 resolved=str(pathlib.Path(driver).resolve(strict=True))
 version=subprocess.run([driver,'--version'],check=True,capture_output=True,text=True,timeout=30).stdout.strip()
 definitions=subprocess.run([driver,'-dM','-E','-x','c','-'],input='',check=True,capture_output=True,text=True,timeout=30).stdout
 macros={}
 for line in definitions.splitlines():
  if line.startswith('#define '):
   key,_,value=line[len('#define '):].partition(' ');macros[key]=value
 clang='__clang__' in macros
 gnu='__GNUC__' in macros and not clang and not any(k in macros for k in ['__INTEL_COMPILER','__INTEL_LLVM_COMPILER'])
 valid=(clang and 'clang' in version.lower()) if implementation=='clang' else (gnu and 'gcc' in version.lower() and 'Free Software Foundation' in version)
 assert valid,f'compiler identity mismatch: {command} requires {implementation}; resolved={resolved}; version={version!r}; __clang__={macros.get("__clang__")}; __GNUC__={macros.get("__GNUC__")}'
 return dict(command=command,driver=driver,resolved_driver=resolved,implementation=implementation,version=version,
             predefines={k:macros[k] for k in ['__clang__','__clang_major__','__clang_minor__','__clang_patchlevel__','__clang_version__','__GNUC__','__GNUC_MINOR__','__GNUC_PATCHLEVEL__','__VERSION__'] if k in macros},
             predefines_sha256=hashlib.sha256(definitions.encode()).hexdigest())
# Probe BOTH implementations before assembly, compilation or any counted run.
assert sys.argv[1:] in ([],['--check-compilers'])
compilers={name:compiler_identity(name,name) for name in ['clang','gcc']}
assert compilers['clang']['resolved_driver']!=compilers['gcc']['resolved_driver'],'compiler drivers resolve to the same executable'
if sys.argv[1:]==['--check-compilers']:
 print(json.dumps(compilers,indent=2));sys.exit(0)
assert platform.system()=='Linux' and platform.machine()=='x86_64','native Linux x86_64 required'
def text(path):
 b=path.read_bytes();off=struct.unpack_from('<Q',b,40)[0];width,count,strings=struct.unpack_from('<HHH',b,58)
 def section(i):return struct.unpack_from('<IIQQQQIIQQ',b,off+i*width)
 sh=section(strings);names=b[sh[4]:sh[4]+sh[5]]
 for i in range(count):
  sh=section(i)
  if names[sh[0]:].split(b'\0',1)[0]==b'.text':return b[sh[4]:sh[4]+sh[5]]
 raise AssertionError('missing text')
expected=bytes.fromhex('d9 fe d9 ff d9 f0 d9 f3 d9 f1 d9 f9 d9 fd d9 f8 d9 f5 df e0 dd d8')
for tool,args in [(compilers['clang']['driver'],['-c']),('as',['--64'])]:
 subprocess.run([tool,*args,str(p/'encoding.S'),'-o',str(p/'encoding.o')],check=True,timeout=60)
 assert text(p/'encoding.o')==expected
results=[]
for compiler,identity in compilers.items():
 driver=identity['driver']
 flags=['-fno-vectorize','-fno-slp-vectorize'] if compiler=='clang' else ['-fno-tree-vectorize']
 for name in names:
  exe=p/(name+'-'+compiler+'.exe')
  subprocess.run([driver,'-O2',*flags,'-mno-red-zone',str(p/'test.c'),str(p/'probe.S'),str(p/(name+'.o')),'-o',str(exe)],check=True,timeout=120)
  r=subprocess.run([str(exe)],capture_output=True,text=True,timeout=120)
  print(compiler,name,r.returncode,r.stdout,r.stderr,flush=True)
  expected='PASS x87-numerical checks=1503936 exact_payloads=654720 domain_diagnostics=96384 raw_status_diagnostics=1503936\n'
  diagnostic=re.fullmatch(r'DIAGNOSTICS undefined_payload_differences=([0-9]+) raw_status_differences=([0-9]+)\n',r.stdout.removeprefix(expected))
  assert r.returncode==0 and r.stdout.startswith(expected) and diagnostic is not None
  assert int(diagnostic[1])<=96384 and int(diagnostic[2])<=1503936
  results.append(dict(compiler=compiler,compiler_identity=identity,object=name,sha256=hashes()[name+'.o'],stdout=r.stdout,diagnostics=dict(undefined_payload_differences=int(diagnostic[1]),raw_status_differences=int(diagnostic[2]))))
assert len(results)==576
(p/'results.json').write_text(json.dumps(results,indent=2)+'\n')
print('PASS all 576 x87-numerical native runs')
