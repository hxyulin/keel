#!/usr/bin/env python3
"""Keep heavyweight proof outputs outside Git, addressed by their SHA256."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / 'artifacts.lock.json'

def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024),b''): h.update(chunk)
    return h.hexdigest()

def store_root():
    configured=os.environ.get('KEEL_ARTIFACT_STORE')
    if configured:return Path(configured).expanduser().resolve()
    base=Path.home()/('Library/Application Support' if sys.platform=='darwin' else '.local/share')
    return base/'keel/artifacts'

def object_path(sha):
    if len(sha)!=64 or any(c not in '0123456789abcdef' for c in sha):raise ValueError('invalid SHA256')
    return store_root()/'sha256'/sha[:2]/sha[2:]

def read_index():
    if not INDEX.exists():return {'schema':1,'artifacts':{}}
    value=json.loads(INDEX.read_text())
    if value.get('schema')!=1 or not isinstance(value.get('artifacts'),dict):raise ValueError('unsupported artifact index')
    return value

def destination(name):
    rel=Path(name)
    if rel.is_absolute() or '..' in rel.parts:raise ValueError('unsafe artifact path: '+name)
    path=ROOT/rel
    for p in [path,*path.parents]:
        if p==ROOT:break
        if p.is_symlink():raise ValueError('symlink artifact path: '+name)
    return path

def verified(path,row):
    return path.is_file() and not path.is_symlink() and path.stat().st_size==row['bytes'] and digest(path)==row['sha256']

def copy_verified(source,dest,row):
    dest.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=dest.parent,prefix='.keel-artifact-',delete=False) as f:tmp=Path(f.name)
    try:
        shutil.copyfile(source,tmp)
        if not verified(tmp,row):raise ValueError('artifact content changed during copy')
        os.replace(tmp,dest)
    finally:tmp.unlink(missing_ok=True)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='command',required=True)
    add=sub.add_parser('add',help='Copy files into the local store and update the tracked index')
    add.add_argument('paths',nargs='+')
    for command in ['restore','verify']:
        p=sub.add_parser(command)
        p.add_argument('paths',nargs='*')
    args=parser.parse_args();index=read_index();rows=index['artifacts']
    if args.command=='add':
        for name in args.paths:
            path=destination(name)
            if not path.is_file() or path.is_symlink():raise ValueError('regular artifact required: '+name)
            row={'sha256':digest(path),'bytes':path.stat().st_size}
            obj=object_path(row['sha256'])
            if obj.exists():
                if not verified(obj,row):raise ValueError('corrupt store object: '+str(obj))
            else:copy_verified(path,obj,row)
            rows[name]=row
        index['artifacts']=dict(sorted(rows.items()))
        raw=json.dumps(index,indent=2)+'\n'
        tmp=INDEX.with_name(INDEX.name+'.tmp');tmp.write_text(raw);os.replace(tmp,INDEX)
        print('Stored/indexed',len(args.paths),'artifacts in',store_root());return
    names=args.paths or list(rows);missing=[];done=0
    for name in names:
        if name not in rows:raise ValueError('artifact not indexed: '+name)
        row=rows[name];path=destination(name)
        if verified(path,row):done+=1;continue
        if path.exists():raise ValueError('existing artifact differs; refusing to overwrite: '+name)
        if args.command=='verify':missing.append(name);continue
        obj=object_path(row['sha256'])
        if not verified(obj,row):missing.append(name);continue
        copy_verified(obj,path,row);done+=1
    if missing:
        print('Unavailable artifacts ('+str(len(missing))+'); restore from KEEL_ARTIFACT_STORE:',file=sys.stderr)
        print('\n'.join(missing),file=sys.stderr);raise SystemExit(1)
    print('Verified',done,'artifacts'+(' (restored missing files)' if args.command=='restore' else ''))

if __name__=='__main__':main()
