"""Compare 256 packed/pragma record layouts and initializers against Clang."""
import argparse
import random
import subprocess
import importlib.util
import json
from pathlib import Path
root=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--keel',type=Path,required=True)
parser.add_argument('--work-dir',type=Path,required=True)
parser.add_argument('--clang',default='clang')
args=parser.parse_args()
spec=importlib.util.spec_from_file_location('inspect',root/'suite/compiler_asm_compat/objinspect.py');ins=importlib.util.module_from_spec(spec);spec.loader.exec_module(ins)
work=args.work_dir.resolve();work.mkdir(parents=True,exist_ok=True)
r=random.Random(0x8617a539)
lines=[]
for n in range(256):
    mode=n%5
    if mode: lines.append(f'#pragma pack(push, {1<<(mode-1)})')
    lines.append(f'struct '+('__attribute__((packed)) ' if mode==0 else '')+f'S{n} {{')
    values=[]
    for j in range(9):
        ty,bits=r.choice([('unsigned char',8),('unsigned short',16),('unsigned int',32),('unsigned long',64)])
        width=r.randint(1,bits)
        if j==3 and n%3==0:
            lines.append('unsigned int :0;')
        if j==5 and n%3==1:
            lines.append('unsigned char pad;');values.append('173')
        lines.append(f'{ty} f{j}:{width};')
        values.append(f'{r.getrandbits(width)}UL')
    lines.append('};')
    if mode: lines.append('#pragma pack(pop)')
    lines.append(f'struct S{n} value{n} = {{'+','.join(values)+'};')
    lines.append(f'unsigned long layout{n}[2] = {{sizeof(struct S{n}), _Alignof(struct S{n})}};')
source=work/'matrix.c';source.write_text('\n'.join(lines)+'\n')
report={}
for target in ['x86_64-unknown-linux-gnu','aarch64-unknown-linux-gnu']:
    paths=[work/(target+'-keel.o'),work/(target+'-clang.o')]
    subprocess.run([args.keel.resolve(),'cc','--target',target,'--emit-object',source,'-o',paths[0]],check=True,capture_output=True)
    subprocess.run([args.clang,'-w','-target',target,'-c',source,'-o',paths[1]],check=True,capture_output=True)
    values=[]
    for p in paths:
        obj=ins.parse(p);out={}
        for name,bind,ty,vis,where,size in obj['symbols']:
            if name.startswith(('value','layout')):
                sect,offset=where
                out[name]=(size,obj['sections'][sect]['data'][offset:offset+size])
        values.append(out)
    for name in values[0]:
        assert values[0][name]==values[1][name],(target,name,values[0][name],values[1][name])
    report[target]=len(values[0])
report={'seed': '0x8617a539', 'records_per_target': 256, 'matched_data_symbols': report}
print(json.dumps(report,indent=2))
(work/'report.json').write_text(json.dumps(report,indent=2)+'\n')
