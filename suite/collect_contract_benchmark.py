"""Collect reports and losslessly gzip raw evidence, without compiler/app binaries."""
import argparse
import hashlib
import json
import pathlib
import shutil
import tarfile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("source", type=pathlib.Path)
    p.add_argument("destination", type=pathlib.Path)
    args = p.parse_args()
    args.destination.mkdir(parents=True, exist_ok=True)
    entries = []
    archive = args.destination / "raw-evidence.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        for path in sorted(args.source.rglob("*")):
            rel = path.relative_to(args.source)
            if not path.is_file() or (rel.parts[0] in ["apps", "avx-apps"] and path.name != "correctness.log"):
                continue
            if path.suffix not in [".json", ".gz", ".log", ".keel", ".ir", ".c", ".rs", ".s", ".dump", ".sh"]:
                continue
            entries.append(dict(path=str(rel), bytes=path.stat().st_size, sha256=sha(path.read_bytes())))
            tar.add(path, arcname=str(rel), recursive=False)
            if path.suffix == ".json":
                dest = args.destination / rel
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, dest)
    # Verify every archived byte against its source hash; never normalize logs.
    with tarfile.open(archive) as tar:
        for entry in entries:
            assert sha(tar.extractfile(entry["path"]).read()) == entry["sha256"]
    manifest = dict(source=str(args.source), archive_sha256=sha(archive.read_bytes()),
                    archive_bytes=archive.stat().st_size, entries=entries)
    (args.destination / "evidence-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
