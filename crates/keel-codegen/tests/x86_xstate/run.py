#!/usr/bin/env python3
"""Sealed native packet: missing, changed, skipped and partial runs fail closed."""
import hashlib,json,pathlib,platform,subprocess,sys,struct,os
p=pathlib.Path(__file__).resolve().parent
if not sys.argv[1:]: (p/'results.json').unlink(missing_ok=True)
if not __debug__: raise SystemExit('optimized Python disables acceptance checks')
names=[f'{s}-{a}-{pool}' for s in ['Baseline','Trees','Full'] for a in ['Fast','Full'] for pool in ['normal','spill','custom']]
assert (p/'objects.txt').read_text().splitlines()==names
files=['README.md','encoding.S','builtin.c','probe.S','test.c','test.ir','run.py','objects.txt']+[f'{n}.{ext}' for n in names for ext in ['o','s']]
def hashes():return {f:hashlib.sha256((p/f).read_bytes()).hexdigest() for f in files}
if sys.argv[1:]==['--seal']:
 (p/'manifest.json').write_text(json.dumps(hashes(),indent=2)+'\n');sys.exit(0)
assert json.loads((p/'manifest.json').read_text())==hashes(),'packet hash mismatch'
for name in names:
 header=(p/(name+'.o')).read_bytes()[:20]
 assert header[:7]==b'\x7fELF\x02\x01\x01' and header[16:20]==b'\x01\x00\x3e\x00','invalid native object'
if sys.argv[1:]==['--check']:print('PASS sealed xstate packet 18 objects');sys.exit(0)
assert not sys.argv[1:]
assert platform.system()=='Linux' and platform.machine()=='x86_64','requires native Linux x86_64'
def text_bytes(path):
 data=path.read_bytes()
 assert data[:7]==b'\x7fELF\x02\x01\x01' and data[16:20]==b'\x01\x00\x3e\x00'
 offset=struct.unpack_from('<Q',data,40)[0]
 width,count,strings=struct.unpack_from('<HHH',data,58)
 def section(i): return struct.unpack_from('<IIQQQQIIQQ',data,offset+i*width)
 sh=section(strings); names=data[sh[4]:sh[4]+sh[5]]
 for i in range(count):
  sh=section(i)
  if names[sh[0]:].split(b'\0',1)[0]==b'.text':return data[sh[4]:sh[4]+sh[5]]
 raise AssertionError('missing .text')
expected=bytes.fromhex('48 0f ae 21 48 0f ae 29 49 0f ae 64 24 40 49 0f ae 6c 24 40')
for tool in ['clang',os.environ.get('KEEL_XSTATE_GNU_AS','as')]:
 command=[tool]+(['-c'] if tool=='clang' else ['--64'])+[str(p/'encoding.S'),'-o',str(p/'encoding.o')]
 subprocess.run(command,check=True,timeout=60)
 assert text_bytes(p/'encoding.o')==expected,'primary assembler encoding mismatch'
subprocess.run(['clang','-O2','-mxsave','-c',str(p/'builtin.c'),'-o',str(p/'builtin.o')],check=True,timeout=60)
assert bytes.fromhex('48 0f ae 27') in text_bytes(p/'builtin.o') and bytes.fromhex('48 0f ae 2f') in text_bytes(p/'builtin.o')
results=[]
for name in names:
 exe=p/(name+'.exe')
 subprocess.run(['clang','-O2','-fno-vectorize','-fno-slp-vectorize','-mavx','-mno-red-zone',str(p/'test.c'),str(p/'probe.S'),str(p/'builtin.o'),str(p/(name+'.o')),'-o',str(exe)],check=True)
 r=subprocess.run([str(exe)],capture_output=True,text=True,timeout=120)
 print(name,r.returncode,r.stdout,r.stderr,flush=True)
 assert r.returncode==0 and r.stdout=='PASS xstate masks=7 patterns=28 live=57344 faults=8 raw=8\n'
 results.append({'object':name,'sha256':hashes()[name+'.o'],'returncode':r.returncode,'stdout':r.stdout})
(p/'results.json').write_text(json.dumps(results,indent=2)+'\n')
assert len(results)==18
print('PASS all 18 xstate native configurations')
