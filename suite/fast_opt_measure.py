#!/usr/bin/env python3
"""Frozen-source Fast tier experiment; invoke through fast_opt_locked.py.

Usage: fast_opt_measure.py ARTIFACT_ROOT stages|checks|extra|bridge|runtime
Requires build/release/{keel,examples/fast_opt_probe}, the existing frozen
codegen investigation, and preprocessed suite inputs. Refuses to overwrite
measurements. Records command arrays; no checkout or production builds here.
"""
import hashlib
import json
import pathlib
import random
import signal
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
R = pathlib.Path(sys.argv[1]).resolve()
OLD = ROOT/'target/cranelift-bridge-20261019/run-r1'
CG = ROOT/'target/codegen-cranelift-investigation-20261019'
PROBE = R/'build/release/examples/fast_opt_probe'
BENCH = CG/'bin/bench-reference'
TIERS = ['none', 'fast', 'basic', 'full']
commands = []


def run(args, **kwargs):
    args = list(map(str, args))
    commands.append(args)
    p = subprocess.run(args, capture_output=True, **kwargs)
    if p.returncode:
        raise RuntimeError((args, p.returncode, p.stderr[-3000:].decode(errors='replace')))
    return p.stdout


def save(path, data):
    with pathlib.Path(path).open('xb') as out:
        out.write(data)


def stages():
    inputs = [OLD/'ir/kernels.raw.keel', OLD/'ir/bridge.keel'] + sorted((OLD/'lua').glob('*.raw.subset.keel'))
    save(R/'stages-raw.jsonl', run([PROBE, '11', R/'raw', *inputs]))
    inputs = [p for app in ['lua','quickjs','sqlite'] for p in sorted((ROOT/f'target/suite/{app}/pre').glob('*.i'))]
    save(R/'stages-c.jsonl', run([PROBE, '7', R/'c', *inputs]))


def checks():
    (R/'bin').mkdir(exist_ok=True)
    (R/'correctness').mkdir(exist_ok=True)
    records = []
    reference = (CG/'correctness/kernels.none.full-all.txt').read_bytes()
    for tier in TIERS:
        exe = R/f'bin/kernels.{tier}'
        run(['clang', CG/'objects/driver.o', R/f'raw/0-kernels.raw.{tier}.o', '-o', exe])
        output = run([exe,'check'])
        assert output == reference, ('kernels', tier)
        save(R/f'correctness/kernels.{tier}.txt', output)
        records.append(dict(kind='kernel', tier=tier, sha256=hashlib.sha256(output).hexdigest(), lines=output.count(b'\n')))
        exe = R/f'bin/bridge.{tier}'
        run(['clang','-O1',CG/'source/crates/keel-cranelift/tests/fixtures/bridge_driver.c', R/f'raw/1-bridge.{tier}.o','-o',exe])
        output = run([exe])
        assert output == (CG/'correctness/bridge.full-baseline.txt').read_bytes()
        traps = {}
        for case in ['sdiv0','udiv0','srem0','urem0','sdivmin','sdiv8min','trap']:
            p = subprocess.run([exe,'trap',case],capture_output=True)
            assert p.returncode == -signal.SIGTRAP, (tier,case,p.returncode)
            traps[case] = p.returncode
        records.append(dict(kind='bridge',tier=tier,sha256=hashlib.sha256(output).hexdigest(),traps=traps))
        for app, binary, libs in [('lua','lua',['-lm']),('quickjs','qjs',['-lm','-lpthread']),('sqlite','sqlite3',['-lm','-lpthread'])]:
            stems = [p.stem for p in (ROOT/f'target/suite/{app}/pre').glob('*.i')]
            objects = [p for p in (R/'c').glob(f'*.{tier}.o') if p.name.split('-',1)[1].rsplit('.',2)[0] in stems]
            assert len(objects)==len(stems),(app,len(objects),len(stems))
            exe = R/f'bin/{app}.{tier}'
            run(['clang', *objects, *libs, '-o', exe])
            if app == 'lua':
                for case in ['fib','sort','string','float']:
                    command=[ROOT/'target/suite/lua/build/clang-O2/lua',ROOT/'suite/lua/bench.lua',case]
                    expected=run(command)
                    output=run([exe,*command[1:]])
                    assert output==expected,(app,tier,case,output,expected)
                    records.append(dict(kind=app,tier=tier,case=case,sha256=hashlib.sha256(output).hexdigest()))
            elif app == 'quickjs':
                # Full upstream suite includes workers and external environment;
                # run the language/builtin/loop/closure/bignum tests here.
                cwd=ROOT/'target/suite/quickjs/quickjs-2024-01-13'
                for case in ['test_closure','test_language','test_builtin','test_loop','test_bignum']:
                    flags=['--bignum'] if case=='test_bignum' else []
                    output=run([exe,*flags,f'tests/{case}.js'],cwd=cwd)
                    records.append(dict(kind=app,tier=tier,case=case,sha256=hashlib.sha256(output).hexdigest()))
            else:
                stdin=(ROOT/'suite/sqlite/queries.sql').read_bytes()
                output=run([exe,':memory:'],input=stdin)
                expected=run([ROOT/'target/suite/sqlite/build/clang-O2/sqlite3',':memory:'],input=stdin)
                assert output==expected,(app,tier)
                records.append(dict(kind=app,tier=tier,sha256=hashlib.sha256(output).hexdigest()))
    save(R/'correctness.json',json.dumps(records,indent=2).encode())


def bridge():
    # Bridge's native rows are from its older frozen code; use only the CLIF
    # rows for candidate comparisons. Both backends consume the saved same IR.
    paths=[p for p in sorted((R/'raw').glob('*.keel')) if '.none.' in p.name or '.fast.' in p.name]
    run([BENCH,'time','--warmup','2','--reps','11','--out',R/'bridge-compile.json',*[f'{p.stem}={p}' for p in paths]])
    run([BENCH,'objects','--out-dir',R/'bridge-objects','--label','kernels.raw',OLD/'ir/kernels.raw.keel'])
    for cfg in ['clif-none','clif-speed']:
        exe=R/f'bin/kernels.{cfg}'
        run(['clang',CG/'objects/driver.o',R/f'bridge-objects/kernels.raw.{cfg}.o','-o',exe])
        assert run([exe,'check'])==(CG/'correctness/kernels.none.full-all.txt').read_bytes()


def extra():
    for tier in ['none','fast']:
        output=run([R/f'bin/lua.{tier}','-e','_U=true','all.lua'],cwd=ROOT/'target/suite/lua/lua-5.4.7-tests')
        save(R/f'correctness/lua-portable.{tier}.txt',output)
        for case,flags in [('test_std',[]),('test_worker',[]),('test_op_overloading',['--bignum']),('test_qjscalc',['--qjscalc'])]:
            output=run([R/f'bin/quickjs.{tier}',*flags,f'tests/{case}.js'],cwd=ROOT/'target/suite/quickjs/quickjs-2024-01-13')
            save(R/f'correctness/quickjs-{case}.{tier}.txt',output)


def runtime():
    rng=random.Random(20261019)
    kernels={'logic':2000,'fnv1a':1000,'crc32':300,'sieve':30,'matmul':20,'dot':3000,'saxpy':3000,'divmix':300,'gcd':20,'isort':10,'qsort':20,'interp':30,'classify':2000,'bits':1000,'narrow':1000,'fconv':300,'collatz':1000,'copy':2000,'dispatch':1000,'names':300,'stackobj':1000}
    samples=[]
    for kernel,reps in kernels.items():
        expected=None
        for round in range(7):
            configs=TIERS+['clif-none','clif-speed'];rng.shuffle(configs)
            for config in configs:
                row=run([R/f'bin/kernels.{config}','time',kernel,reps]).decode().split()
                if expected is None:expected=row[2]
                assert row[2]==expected,(kernel,config,row[2],expected)
                samples.append(dict(kernel=kernel,config=config,round=round,reps=reps,ns=int(row[1]),checksum=row[2]))
    save(R/'runtime.json',json.dumps(samples,indent=2).encode())
    app_samples=[]
    for app,case in [('lua','fib'),('lua','sort'),('lua','string'),('quickjs','fib'),('quickjs','sort'),('quickjs','string')]:
        expected=None
        for round in range(5):
            configs=TIERS[:];rng.shuffle(configs)
            for config in configs:
                script=ROOT/f'suite/{app}/bench.{"lua" if app=="lua" else "js"}'
                flags=['--stack-size','4194304'] if app=='quickjs' else []
                start=time.perf_counter_ns()
                output=run([R/f'bin/{app}.{config}',*flags,script,case])
                elapsed=time.perf_counter_ns()-start
                if expected is None:expected=output
                assert output==expected,(app,case,config)
                app_samples.append(dict(app=app,case=case,config=config,round=round,ns=elapsed,sha256=hashlib.sha256(output).hexdigest()))
    save(R/'runtime-apps.json',json.dumps(app_samples,indent=2).encode())


if __name__=='__main__':
    mode=sys.argv[2]
    if mode not in ['stages','checks','extra','bridge','runtime']:
        raise SystemExit(__doc__)
    if (R/f'commands-{mode}.json').exists():
        raise SystemExit('Use a fresh artifact directory; this mode already ran.')
    try:
        globals()[mode]()
    finally:
        save(R/f'commands-{mode}.json',json.dumps(commands,indent=2).encode())
