"""Freeze a compiler revision with only width-comparison helpers overlaid.

python3 suite/width-comparison/provenance.py --base REV --output /tmp/SNAPSHOT
The manifest is verified again by the runner before any compilation or timing.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[2]
EXAMPLE = 'crates/keel/examples/width_comparison.rs'


def blob_id(data):
    return hashlib.sha1(f'blob {len(data)}\0'.encode() + data).hexdigest()


def verify(root, manifest):
    if len(manifest['base']) != 40:
        raise ValueError('full frozen revision required')
    overlays = manifest['overlays']
    if EXAMPLE not in overlays or any(
            name != EXAMPLE and not name.startswith('suite/width-comparison/')
            for name in overlays):
        raise ValueError('overlay outside dedicated helpers')
    for name, expected in manifest['blobs'].items():
        if name not in overlays and blob_id((root/name).read_bytes()) != expected:
            raise ValueError(f'frozen base changed: {name}')
    for name, expected in overlays.items():
        if blob_id((root/name).read_bytes()) != expected:
            raise ValueError(f'helper changed after freezing: {name}')
    if (root/EXAMPLE).read_bytes() != (root/'suite/width-comparison/compiler.rs').read_bytes():
        raise ValueError('example differs from maintained compiler helper')
    return manifest['base']


def freeze(base, output):
    def git(*args):
        return subprocess.check_output(['git', '-C', str(ROOT), *args])
    base = git('rev-parse', f'{base}^{{commit}}').decode().strip()
    blobs = {}
    for record in git('ls-tree', '-rz', base).split(b'\0'):
        if record:
            metadata, name = record.split(b'\t')
            mode, kind, oid = metadata.split()
            if kind != b'blob' or mode not in [b'100644', b'100755']:
                raise ValueError(f'unsupported snapshot entry: {name!r}')
            blobs[name.decode()] = oid.decode()
    output.mkdir(parents=True, exist_ok=False)
    with tarfile.open(fileobj=io.BytesIO(git('archive', base))) as archive:
        for name in blobs:
            if Path(name).is_absolute() or '..' in Path(name).parts:
                raise ValueError(f'unsafe archive path: {name}')
            path = output/name
            path.parent.mkdir(parents=True, exist_ok=True)
            member = archive.getmember(name)
            path.write_bytes(archive.extractfile(member).read())
            path.chmod(member.mode & 0o777)
    for name, expected in blobs.items():
        if blob_id((output/name).read_bytes()) != expected:
            raise ValueError(f'archive differs from Git: {name}')
    overlays = {}
    for source in sorted((ROOT/'suite/width-comparison').iterdir()):
        if source.suffix in ['.py', '.rs', '.c'] and source.is_file():
            name = source.relative_to(ROOT).as_posix()
            (output/name).write_bytes(source.read_bytes())
            overlays[name] = blob_id(source.read_bytes())
    (output/EXAMPLE).parent.mkdir(parents=True, exist_ok=True)
    (output/EXAMPLE).write_bytes((output/'suite/width-comparison/compiler.rs').read_bytes())
    overlays[EXAMPLE] = blob_id((output/EXAMPLE).read_bytes())
    manifest = dict(base=base, blobs=blobs, overlays=overlays)
    verify(output, manifest)
    (output/'base-manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print(json.dumps(dict(base=base, base_blobs=len(blobs), overlays=list(overlays), output=str(output))))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    freeze(args.base, args.output.resolve())
