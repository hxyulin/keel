#!/usr/bin/env python3
"""Keel pattern lab: stdlib-only entry point, never benchmarks by default."""
import sys
sys.dont_write_bytecode = True
import argparse
import json
from pathlib import Path
from lab.manifest import LabError, load
from lab.cancel import TerminationRequested, ignore_cancellation, signal_handlers, raise_cancel
from lab.engine import Runner, build, check, bench, seal, write, verify_record, hostarch

ROOT=Path(__file__).resolve().parent

def _main(argv=None):
    p=argparse.ArgumentParser(description=__doc__)
    sub=p.add_subparsers(dest='command',required=True)
    for cmd in ('list','check','bench'):
        c=sub.add_parser(cmd)
        c.add_argument('manifests',nargs='*',type=Path,help='manifest paths; default patterns/*.json')
        if cmd!='list':
            c.add_argument('--output',required=True,type=Path,help='new output directory (never reused)')
            c.add_argument('--clang',default='clang',help='Clang executable path or PATH name')
            c.add_argument('--timeout',type=int,default=20,help='per-process seconds, 1..60')
            c.add_argument('--budget',type=int,default=180,help='total build/check/campaign deadline seconds, 1..600')
        if cmd=='bench':
            c.add_argument('--execute',action='store_true',help='required explicit benchmark opt-in')
            c.add_argument('--timer',choices=('monotonic','tsc'),default='monotonic')
            c.add_argument('--target-ms',type=int,default=100)
            c.add_argument('--pilot-iterations',type=int,default=100000)
            c.add_argument('--max-iterations',type=int,default=1000000000)
            c.add_argument('--cycles',type=int,default=2,help='complete permutation cycles, 2..10')
    v=sub.add_parser('verify'); v.add_argument('output',type=Path)
    a=p.parse_args(argv)
    r=None; evidence=None
    try:
        if a.command=='verify':
            path=a.output.resolve()/'integrity.json'
            record=json.loads(path.read_text())
            if not isinstance(record,dict) or not record: raise LabError('invalid integrity record')
            errors=verify_record(record)
            print(json.dumps({'status':'changed' if errors else 'verified','mutations':errors},indent=2)); return 1 if errors else 0
        selected=[(x.resolve(),load(x,ROOT)) for x in (a.manifests or sorted((ROOT/'patterns').glob('*.json')))]
        if not selected: raise LabError('no manifests selected')
        ids=[m['id'] for _,m in selected]
        if len(set(ids))!=len(ids): raise LabError('duplicate pattern id')
        if a.command=='list':
            print(json.dumps([{'id':m['id'],'arch':m['arch'],'status':'native' if m['arch']==hostarch() else 'unsupported','category':m['category'],'variants':[v['id'] for v in m['variants']]} for _,m in selected],indent=2)); return 0
        if not 1<=a.timeout<=60 or not 1<=a.budget<=600: raise LabError('timeout/budget out of bounds')
        if a.command=='bench':
            if not a.execute: raise LabError('bench requires explicit --execute')
            if not 1<=a.pilot_iterations<=a.max_iterations<=1000000000 or not 100<=a.target_ms<=250 or not 2<=a.cycles<=10:
                raise LabError('benchmark bounds: 1 <= pilot <= max <= 1000000000, target 100..250 ms, cycles 2..10')
        r=Runner(a.output,a.timeout,a.budget)
        write(r.output/'selection.json',{'manifests':[str(x) for x,_ in selected],'command':a.command})
        evidence=build(r,ROOT,selected,a.clang)
        check(r,evidence)
        write(r.output/'build.json',evidence)
        native=sum(row['status']=='checked' for row in evidence['patterns'])
        if a.command=='bench' and native: bench(r,evidence,a)
        status='checked' if native else 'unsupported'
        if a.command=='bench' and native: status='measured'
        write(r.output/'status.json',{'status':status,'native_patterns':native,'unsupported_patterns':len(selected)-native,'automatic_profit_pass':False})
        seal(r,evidence)
        print(json.dumps({'status':status,'native_patterns':native,'unsupported_patterns':len(selected)-native,'output':str(r.output)}))
        return 0 if native else 3
    except KeyboardInterrupt as e:
        code=143 if isinstance(e,TerminationRequested) else 130
        with ignore_cancellation():
            if r:
                evidence=evidence or r.evidence
                write(r.output/'failure.json',{'status':'cancelled','signal':'SIGTERM' if code==143 else 'SIGINT','exit_code':code})
                if evidence and not (r.output/'build.json').exists(): write(r.output/'build.json',evidence)
                if not (r.output/'integrity.json').exists(): seal(r,evidence)
            print('campaign cancelled',file=sys.stderr)
        return code
    except (LabError,OSError,ValueError) as e:
        if r:
            evidence=evidence or r.evidence
            write(r.output/'failure.json',{'status':'failed','error':str(e)})
            if evidence and not (r.output/'build.json').exists(): write(r.output/'build.json',evidence)
            seal(r,evidence)
        print(str(e),file=sys.stderr); return 1

def main(argv=None):
    with signal_handlers(raise_cancel):
        return _main(argv)

if __name__=='__main__':
    raise SystemExit(main())
