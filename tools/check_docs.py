#!/usr/bin/env python3
"""Check links, anchors and repository paths in tracked Markdown files.

Every relative link must name a tracked file or directory, and a `#fragment`
on a Markdown target must name a heading or an explicit `<a id>` there.
Backticked paths that start with a top-level tracked directory, such as
`crates/keel-opt/src/lib.rs`, must exist from the repository root or from the
page's directory, or be ignored by Git. Every page outside `tests/` directories,
apart from AGENTS.md, must be reachable by links from docs/README.md. Fenced code
blocks are ignored.

Usage: check_docs.py [--root DIR]. Exits 1 and lists each problem as
`file:line: message` when any check fails."""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import unicodedata
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
INDEX = "docs/README.md"
# Pages that need no link from the index: contributor rules and test fixture notes.
UNINDEXED = re.compile(r"AGENTS\.md$|(?:^|.*/)tests/")
# Backticked tokens under these directories are checked as repository paths.
PATH_ROOTS = ("crates/", "docs/", "suite/", "tools/", "third-party/")
# A token with one of these characters is a pattern or placeholder, not a path.
PLACEHOLDER = re.compile(r"[<>*{}$?\[\] ]|\.\.\.")
LINK = re.compile(r"(?<!\\)!?\[(?:[^\[\]]|\[[^\]]*\])*\]\(\s*<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\s*\)")
REFERENCE = re.compile(r"^\s{0,3}\[[^\]]+\]:\s+<?(\S+?)>?(?:\s+.*)?$")
ANCHOR = re.compile(r"<a\s+(?:id|name)=\"([^\"]+)\"")
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
CODE_SPAN = re.compile(r"(`+)(.+?)\1")
FENCE = re.compile(r"^\s{0,3}(`{3,}|~{3,})")


def tracked(root: Path) -> list[str]:
    output = subprocess.run(["git", "ls-files", "-z"], cwd=root, check=True,
                            capture_output=True).stdout.decode()
    return [path for path in output.split("\0") if path]


def prose_lines(text: str):
    """Yield (line number, line) outside fenced code blocks."""
    fence = None
    for number, line in enumerate(text.splitlines(), 1):
        match = FENCE.match(line)
        if fence:
            if match and match.group(1)[0] == fence[0] and len(match.group(1)) >= len(fence):
                fence = None
            continue
        if match:
            fence = match.group(1)
            continue
        yield number, line


def slug(heading: str) -> str:
    """GitHub's heading anchor: rendered text, lowercased, punctuation removed."""
    text = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", heading)
    text = re.sub(r"<[^>]+>", "", text)
    text = text.replace("`", "").replace("*", "")
    text = re.sub(r"(?<!\w)_+|_+(?!\w)", "", text)
    text = text.strip().lower()
    kept = "".join(ch for ch in text
                   if ch in " -_" or unicodedata.category(ch)[0] in "LNM")
    return kept.replace(" ", "-")


def anchors(text: str) -> set[str]:
    found: set[str] = set()
    counts: dict[str, int] = {}
    for _, line in prose_lines(text):
        found.update(ANCHOR.findall(line))
        match = HEADING.match(line)
        if match:
            base = slug(match.group(2))
            count = counts.get(base, 0)
            counts[base] = count + 1
            found.add(base if count == 0 else f"{base}-{count}")
    return found


def links(line: str) -> list[str]:
    """Link targets on a line, ignoring code spans."""
    line = CODE_SPAN.sub(lambda match: " " * len(match.group(0)), line)
    targets = LINK.findall(line)
    reference = REFERENCE.match(line)
    if reference:
        targets.append(reference.group(1))
    return targets


def code_paths(line: str) -> list[str]:
    paths = []
    for _, span in CODE_SPAN.findall(line):
        token = span.strip()
        if not token.startswith(PATH_ROOTS) or PLACEHOLDER.search(token):
            continue
        # Drop a trailing `:line`, `:line-line` or `#anchor` reference.
        token = re.sub(r"(?::\d+(?:-\d+)?|#.*)$", "", token).rstrip("/")
        paths.append(token)
    return paths


def ignored(root: Path, paths: set[str]) -> set[str]:
    if not paths:
        return set()
    # A directory pattern such as `/suite/reports/` matches only with the slash.
    candidates = sorted(paths | {path + "/" for path in paths})
    result = subprocess.run(["git", "check-ignore", "--no-index", "--stdin"], cwd=root,
                            input="\n".join(candidates), capture_output=True, text=True)
    return {path.rstrip("/") for path in result.stdout.split()}


def normalize(path: PurePosixPath) -> str | None:
    parts: list[str] = []
    for part in path.parts:
        if part == "..":
            if not parts:
                return None
            parts.pop()
        elif part != ".":
            parts.append(part)
    return "/".join(parts)


def check(root: Path) -> list[str]:
    files = tracked(root)
    existing = set(files)
    directories = {str(parent) for path in files for parent in PurePosixPath(path).parents}
    pages = [path for path in files if path.endswith(".md")]
    texts = {page: (root / page).read_text(encoding="utf-8") for page in pages}
    anchor_sets = {page: anchors(text) for page, text in texts.items()}
    problems = []
    unknown: list[tuple[str, int, str]] = []
    edges: dict[str, set[str]] = {page: set() for page in pages}

    for page, text in texts.items():
        base = PurePosixPath(page).parent
        for number, line in prose_lines(text):
            for target in links(line):
                if re.match(r"[a-z][a-z0-9+.-]*:", target, re.I):
                    continue
                path, _, fragment = target.partition("#")
                if not path:
                    resolved = page
                else:
                    resolved = normalize(base / path)
                    if resolved is None:
                        problems.append(f"{page}:{number}: link leaves the repository: {target}")
                        continue
                if resolved not in existing and resolved not in directories:
                    problems.append(f"{page}:{number}: missing link target: {target}")
                    continue
                if resolved in texts:
                    edges[page].add(resolved)
                    if fragment and fragment not in anchor_sets[resolved]:
                        problems.append(f"{page}:{number}: missing anchor: {target}")
            for path in code_paths(line):
                local = normalize(base / path)
                if not {path, local} & (existing | directories):
                    unknown.append((page, number, path))
    allowed = ignored(root, {path for _, _, path in unknown})
    problems += [f"{page}:{number}: missing path: {path}"
                 for page, number, path in unknown if path not in allowed]

    if INDEX in texts:
        reached, pending = {INDEX}, [INDEX]
        while pending:
            for target in edges[pending.pop()] - reached:
                reached.add(target)
                pending.append(target)
        for page in pages:
            if not UNINDEXED.match(page) and page not in reached:
                problems.append(f"{page}:1: not reachable from {INDEX}")
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--root", type=Path, default=ROOT, help="repository root")
    args = parser.parse_args(argv)
    problems = check(args.root)
    for problem in problems:
        print(problem)
    if problems:
        print(f"{len(problems)} documentation problem(s)", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
