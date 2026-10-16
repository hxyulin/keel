"""Seal the completed private remote experiment, retaining original raw data."""
import gzip,hashlib,json,os,pathlib,subprocess,tarfile
from profile_frontend_refresh import ROOT,BASE,PROD,PERF,ENV,sha,run,store,snapshot,rawinputs,flags

def main():
    r={'base':BASE,'harness_head':os.environ.get('KEEL_FE_HARNESS','unknown'),'final_observation':snapshot(),'production_binary':{'path':str(PROD),'sha256':sha(PROD)},'toolchain':run(['/home/hxyulin/.cargo/bin/rustc','-Vv']),'clang':run(['clang','--version']),'perf':run([PERF,'--version'],env=ENV),'perf_sha256':sha(PERF),'libc_sha256':sha('/usr/lib/libc.so.6'),'source_pristine':{},'source_extra_files':[],'input_manifest':{},'binaries':{},'perf_loss_audit':{}}
    with tarfile.open(ROOT/'source.tar.gz') as archive:
        expected={}
        for m in archive:
            if m.isfile():
                expected[m.name]=hashlib.sha256(archive.extractfile(m).read()).hexdigest()
        r['source_pristine']={'archived_revision':BASE,'archive_sha256':sha(ROOT/'source.tar.gz'),'expected_sha256':expected,'mismatches':[p for p,h in expected.items() if sha(ROOT/'source'/p)!=h]}
        assert not r['source_pristine']['mismatches']
        r['source_extra_files']=[str(p.relative_to(ROOT/'source')) for p in (ROOT/'source').rglob('*') if p.is_file() and str(p.relative_to(ROOT/'source')) not in expected]
    for folder in ['inputs','native-pre']:
        r['input_manifest'][folder]={str(p.relative_to(ROOT)): {'sha256':sha(p),'bytes':p.stat().st_size} for p in (ROOT/folder).rglob('*.i')}
    # Retain only sources and headers from the copied suite, not built application
    # executables. They were never executed during this compile-time campaign.
    r['input_manifest']['raw']={str(p.relative_to(ROOT)):{'sha256':sha(p),'bytes':p.stat().st_size} for p in (ROOT/'raw').rglob('*') if p.is_file() and p.suffix in ['.c','.h']}
    r['ir_manifest']={str(p.relative_to(ROOT)):sha(p) for p in (ROOT/'evidence/ir').rglob('*.ir')}
    for target in ['plain','stage','fp','pp-fp','pp-alloc','memory','diagnostic','storage']:
        td=ROOT/f'target-{target}/release'
        bins=[td/'keel']+list((td/'examples').glob('*'))
        r['binaries'][target]={str(p.relative_to(ROOT)):sha(p) for p in bins if p.is_file() and (p.name in ['profile','resident','resident_memory','profile_allocations','preprocess_linux','preprocess_allocations_linux','keel'])}
        r.setdefault('cargo_fingerprints',{})[target]={str(p.relative_to(ROOT)):json.loads(p.read_text()) for p in (td/'.fingerprint').glob('keel*/**/*.json')}
    for p in sorted((ROOT/'evidence').glob('*.perf.data')):
        if not p.stat().st_size:continue
        # Raw perf stream audit keeps LOST records, decode errors, event attrs,
        # record count; raw data itself is retained without filtering.
        x=run(['sudo','-n','env','LD_LIBRARY_PATH='+ENV['LD_LIBRARY_PATH'],PERF,'script','-f','-D','-i',p],env=ENV)
        dest=p.with_suffix('.raw.txt.gz')
        dest.write_bytes(gzip.compress(x['stdout'].encode(),mtime=0))
        r['perf_loss_audit'][p.name]={'raw_sha256':sha(p),'audit_sha256':sha(dest),'stderr':x['stderr'],'lost_lines':[s for s in x['stdout'].splitlines() if 'PERF_RECORD_LOST' in s or 'LOST_SAMPLES' in s]}
    r['harness_versions']={'resident':'830a8a8','cli_and_preprocessing_timing':'efd8917','resident_sampling':'5195636','preprocess_sampling':'4cac1ee','initializer_histogram_cli_diagnostics':'c280b9f','storage_counts':'f319b22','preprocess_allocations':'4516e41'}
    r['preprocessor_output_parity']=[]
    for app,ps in rawinputs().items():
        for p in ps:
            dest=ROOT/'evidence/production-preprocessed.i'
            result=run([PROD,'cc',p,*flags(app),'-E','--sysroot','/','-o',dest])
            helper=ROOT/'native-pre'/app/p.with_suffix('.i').name
            same=sha(dest)==sha(helper)
            r['preprocessor_output_parity'].append({'path':str(p),'production_sha256':sha(dest),'helper_sha256':sha(helper),'equal':same,**result})
            assert same,p
    store('provenance',r)
    # Keep IR files remote (1.9 GiB); retain every hash locally. All raw samples,
    # event streams, original logs, helpers and source-generators travel losslessly.
    paths=[p for p in (ROOT/'evidence').iterdir() if p.is_file() and p.name!='seal.json']+list(ROOT.glob('*.log'))+list(ROOT.glob('*.txt'))+list(ROOT.glob('*.py'))+[ROOT/'source.tar.gz']
    with tarfile.open(ROOT/'raw-evidence.tar.gz','w:gz') as archive:
        for p in sorted(paths):archive.add(p,arcname=str(p.relative_to(ROOT)))
    with tarfile.open(ROOT/'corpus.tar.gz','w:gz') as archive:
        for folder in ['inputs','native-pre','raw']:
            for p in (ROOT/folder).rglob('*'):
                if p.is_file() and (folder!='raw' or p.suffix in ['.c','.h']):archive.add(p,arcname=str(p.relative_to(ROOT)))
    (ROOT/'LEASE').write_text('RELEASED '+r['final_observation']['time']['stdout'].strip()+'; all owned compile/profile jobs exited\n')
    store('seal',{'raw_archive_sha256':sha(ROOT/'raw-evidence.tar.gz'),'corpus_archive_sha256':sha(ROOT/'corpus.tar.gz'),'lease':(ROOT/'LEASE').read_text(),'final_processes':run(['ps','-eo','pid,ppid,pcpu,etime,args','--sort=-pcpu'])})
if __name__=='__main__':main()
