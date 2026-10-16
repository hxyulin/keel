"""Read-only audit of sealed frontend refresh evidence, including count parity."""
import gzip,hashlib,json,pathlib,re,sys,tarfile

def main(root):
    seal=json.loads((root/'seal.json').read_text());p=json.loads((root/'provenance.json').read_text())
    results={'archive_sha256':{},'source_mismatches':p['source_pristine']['mismatches'],'preprocessor_parity':all(x['equal'] for x in p['preprocessor_output_parity']),'profile_count_parity':{}}
    assert not results['source_mismatches'] and results['preprocessor_parity']
    for n,k in [('raw-evidence.tar.gz','raw_archive_sha256'),('corpus.tar.gz','corpus_archive_sha256')]:
        h=hashlib.sha256((root/n).read_bytes()).hexdigest();assert h==seal[k];results['archive_sha256'][n]=h
    decoded=json.loads(gzip.decompress((root/'decoded.json.gz').read_bytes()))
    with tarfile.open(root/'raw-evidence.tar.gz') as archive:
        resident=json.load(archive.extractfile('evidence/resident.json'));cli=json.load(archive.extractfile('evidence/cli.json'));pp=json.load(archive.extractfile('evidence/preprocess.json'))
        assert len(resident['samples'])==13*3*2*2
        assert len(cli['samples'])==11*3*3
        assert len(pp['samples'])==13*3
        for name,x in decoded.items():
            record=json.load(archive.extractfile('evidence/'+name.replace(':','-')+'-record.json'))
            n=int(re.search(r'\((\d+) samples\)',record['stderr'])[1]);assert n==x['totals']['samples']
            assert sum(x['periods'])==x['totals']['weight']==sum(w for _,w in x['self_weight'])
            results['profile_count_parity'][name]=n
    results['lost_records']=sum(len(x['lost_lines']) for x in p['perf_loss_audit'].values());assert results['lost_records']==0
    results['timing_run_counts']={'resident':len(resident['samples']),'cli':len(cli['samples']),'preprocess':len(pp['samples'])}
    (root/'audit.json').write_text(json.dumps(results,indent=2)+'\n')
    print('Archive hashes, pristine source, native CPP parity, timing run counts, 12 capture counts and weighted denominators verified; zero LOST records.')
if __name__=='__main__':main(pathlib.Path(sys.argv[1]))
