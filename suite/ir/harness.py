"""Shared scaffolding for the LLVM import validators in this directory.

A validator imports LLVM IR with keel-llvm-import, compiles the result with
Keel at each optimization tier and links it against a Clang-built driver. The
expected output comes from the same LLVM IR compiled by Clang. Rejection checks
confirm that a failed import writes nothing and leaves the destination alone.

Each validator takes --libllvm, --keel, --out and optionally --importer (by
default the keel-llvm-import next to --keel). It writes report.json, with every
command and its captured output, into the new --out directory.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import traceback

ROOT = Path(__file__).resolve().parents[2]
TIERS = ('none', 'fast', 'full')
HEADER = ('target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"\n'
          'target triple = "arm64-apple-macosx11.0.0"\n')
SENTINEL = b'llvm import destination preservation sentinel\n'


class Failed(Exception):
    pass


def require(condition, detail):
    if not condition:
        raise Failed(detail if isinstance(detail, str) else repr(detail))


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def llvm_functions(text):
    """Names of LLVM function definitions and declarations, in order."""
    if isinstance(text, bytes):
        text = text.decode()
    return re.findall(r'^(?:define|declare)\b[^@\n]*@([\w.$]+)\(', text, re.M)


def keel_functions(text):
    """Names of Keel functions and declarations, in order."""
    if isinstance(text, bytes):
        text = text.decode()
    return re.findall(r'^(?:func|decl) @([\w.$]+)\(', text, re.M)


def classify_import(returncode, stdout, stderr, preserved):
    """Only known unsupported diagnostics count as coverage blockers."""
    if returncode == 0:
        return 'accepted' if not stdout and not preserved else 'import_protocol_failure'
    if not preserved or stdout:
        return 'destination_preservation_failure'
    if returncode is None:
        return 'import_timeout_or_unavailable'
    if returncode != 1:
        return 'import_crash' if returncode < 0 else 'import_failure'
    # Parser errors, Keel verifier failures, missing APIs and library failures
    # must not be hidden as expected feature rejections.
    diagnostic = stderr.removeprefix('llvm-import: ').strip()
    known = ('unsupported', 'not supported', 'single-block', 'multiple blocks',
             'only single', 'global data', 'globals', 'nsw', 'nuw', 'poison',
             'ordinary C', 'supported types', 'personality', 'intrinsic',
             'stack allocation', 'non-C calling convention', 'variadic signature',
             'i1 ABI', 'no-wrap', 'flagged')
    if (stderr.startswith('llvm-import: ') and
            'Keel verification failed' not in diagnostic and
            not re.search(r'input\.ll:\d+:\d+:', diagnostic) and
            any(word in diagnostic for word in known)):
        return 'rejected'
    return 'import_failure'


class Completed:
    """Exit status and output of one logged command."""

    def __init__(self, record, stdout, stderr):
        self.record, self.stdout, self.stderr = record, stdout, stderr
        self.returncode = record['returncode']


class Runner:
    """Run commands, log each one and keep its output under the artifact directory."""

    def __init__(self, out, timeout):
        self.out, self.timeout, self.commands = out, timeout, []

    def run(self, command, data=None, cwd=None):
        argv = list(map(str, command))
        index = len(self.commands)
        record = {'argv': argv, 'cwd': str(cwd or ROOT)}
        self.commands.append(record)
        prefix = self.out / f'command-{index:04d}'
        record.update(stdout=str(prefix) + '.stdout', stderr=str(prefix) + '.stderr')
        try:
            result = subprocess.run(argv, cwd=cwd or ROOT, input=data, capture_output=True,
                                    timeout=self.timeout)
            stdout, stderr = result.stdout, result.stderr
            record['returncode'] = result.returncode
        except subprocess.TimeoutExpired as e:
            stdout, stderr = e.stdout or b'', e.stderr or b''
            record.update(returncode=None, failure='timeout')
        except OSError as e:
            stdout, stderr = b'', str(e).encode()
            record.update(returncode=None, failure='unavailable')
        Path(record['stdout']).write_bytes(stdout)
        Path(record['stderr']).write_bytes(stderr)
        return Completed(record, stdout, stderr)

    def require(self, command, data=None):
        result = self.run(command, data)
        if result.returncode != 0:
            tail = result.stderr.decode(errors='replace')[-2000:]
            raise Failed(f"command failed ({result.returncode}): {' '.join(result.record['argv'])}\n"
                         f"see {result.record['stderr']}\n{tail}")
        return result


class Importer:
    """keel-llvm-import with output-preservation checks."""

    def __init__(self, runner, path, libllvm, keel, out):
        self.runner, self.path, self.libllvm, self.keel, self.out = runner, path, libllvm, keel, out

    def command(self, source, dest=None, *, keel=None, localize=False):
        argv = [self.path, '--libllvm', self.libllvm, '--keel', keel or self.keel]
        if localize:
            argv.append('--localize-odr-functions')
        argv.append(source)
        if dest is not None:
            argv += ['-o', dest]
        return argv

    def run(self, source, dest=None, *, data=None, keel=None, localize=False):
        return self.runner.run(self.command(source, dest, keel=keel, localize=localize), data)

    def accept(self, source, dest, *, localize=False):
        """Import to dest, check that stdin to stdout gives the same text, return it."""
        source, dest = Path(source), Path(dest)
        original = source.read_bytes()
        dest.write_bytes(SENTINEL)
        result = self.run(source, dest, localize=localize)
        require(result.returncode == 0 and not result.stdout,
                f'import failed: {source}: {result.stderr.decode(errors="replace")}')
        require(source.read_bytes() == original, f'input changed: {source}')
        text = dest.read_bytes()
        require(text != SENTINEL, f'destination not replaced: {dest}')
        streamed = self.run('-', data=original, localize=localize)
        require(streamed.returncode == 0 and streamed.stdout == text,
                f'stdin/stdout import differs: {source}')
        return text.decode()

    def reject(self, source, *, keel=None, localize=False):
        """Require a clean failure for an existing, an absent and a stdout destination.

        Returns the diagnostic text.
        """
        source = Path(source)
        original = source.read_bytes()
        existing, absent = self.out / 'preserved.keel', self.out / 'absent.keel'
        existing.write_bytes(SENTINEL)
        absent.unlink(missing_ok=True)
        diagnostic = None
        for dest in (existing, absent, None):
            result = self.run(source, dest, keel=keel, localize=localize)
            stderr = result.stderr.decode(errors='replace')
            require(result.returncode == 1 and not result.stdout and stderr.startswith('llvm-import:'),
                    f'expected a rejection: {source} -> {dest}: {result.returncode} {stderr}')
            require(existing.read_bytes() == SENTINEL, f'rejection changed the destination: {source}')
            require(not absent.exists(), f'rejection created the destination: {source}')
            require(source.read_bytes() == original, f'input changed: {source}')
            diagnostic = diagnostic or stderr
        return diagnostic


class Context:
    """Paths, tools and results shared by one validator run."""

    def __init__(self, args, out, timeout):
        self.args, self.out = args, out
        self.clang, self.clangxx = args.clang, args.clangxx
        self.run = Runner(out, timeout)
        self.checks = []
        self.keel = Path(args.keel).resolve()
        self.importer_path = Path(args.importer or self.keel.with_name('keel-llvm-import')).resolve()
        libllvm = str(Path(args.libllvm).resolve())
        self.importer = Importer(self.run, self.importer_path, libllvm, self.keel, out)

    def check(self, **fields):
        self.checks.append(fields)

    def compile(self, text, obj, tier, *flags):
        """Compile Keel IR text to an object at one optimization tier."""
        self.run.require([self.keel, 'compile', *flags, '--opt-tier', tier,
                          '--emit-object', text, '-o', obj])
        return obj

    def output(self, exe, *arguments):
        return self.run.require([exe, *arguments]).stdout

    def link_and_run(self, compiler, objects, exe):
        self.run.require([compiler, *objects, '-o', exe])
        return self.output(exe)

    def tiers(self, name, text, objects, expected, *, compiler=None, **fields):
        """Compile text at every tier, link it with objects and compare the output."""
        for tier in TIERS:
            obj = self.compile(text, self.out / f'{name}.{tier}.o', tier)
            actual = self.link_and_run(compiler or self.clang, [*objects, obj],
                                       self.out / f'{name}.{tier}')
            require(actual == expected, f'{name}: {tier} output differs from the reference')
            self.check(kind='native', case=name, tier=tier, **fields)

    def module(self, name, body, header=HEADER):
        """Write a small LLVM module for a rejection case."""
        path = self.out / f'reject-{name}.ll'
        path.write_text(header + body + '\n')
        return path

    def reject_all(self, cases, header=HEADER, *, localize=False, markers=None):
        """Reject every body in cases; markers maps a case to required diagnostic text."""
        for name, body in cases.items():
            diagnostic = self.importer.reject(self.module(name, body, header), localize=localize)
            expected = (markers or {}).get(name, ())
            expected = (expected,) if isinstance(expected, str) else expected
            require(not expected or any(marker in diagnostic for marker in expected),
                    f'{name}: diagnostic lacks {expected}: {diagnostic}')
            self.check(kind='rejection', case=name, diagnostic=diagnostic.strip())

    def verifier_failures(self, source):
        """A failing or missing downstream verifier must leave every destination alone."""
        for verifier in ('/usr/bin/false', self.out / 'missing-keel'):
            diagnostic = self.importer.reject(source, keel=verifier)
            if verifier == '/usr/bin/false':
                require('Keel verification failed' in diagnostic, diagnostic)
            self.check(kind='verifier-failure-preservation', verifier=str(verifier))


def main(validate, doc, *, timeout=120, argv=None):
    """Parse the common options, run validate(context) and write report.json."""
    parser = argparse.ArgumentParser(description=doc,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--libllvm', required=True, help='LLVM 23 shared library')
    parser.add_argument('--keel', required=True, help='keel binary')
    parser.add_argument('--importer', help='keel-llvm-import binary; default: next to --keel')
    parser.add_argument('--out', required=True, help='new artifact directory')
    parser.add_argument('--clang', default='clang')
    parser.add_argument('--clangxx', default='clang++')
    args = parser.parse_args(argv)
    out = Path(args.out).resolve()
    try:
        out.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        parser.error('--out must not already exist')
    context = Context(args, out, timeout)
    report = {'status': 'failed', 'checks': context.checks, 'commands': context.run.commands}
    try:
        report['keel'] = {'path': str(context.keel), 'sha256': digest(context.keel)}
        report['importer'] = {'path': str(context.importer_path),
                              'sha256': digest(context.importer_path)}
        report['libllvm'] = context.importer.libllvm
        result = validate(context)
        if isinstance(result, dict):
            report.update(result)
        report['status'] = 'passed'
    except (Failed, AssertionError, OSError) as error:
        report['error'] = str(error)
        traceback.print_exc()
    finally:
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'status': report['status'], 'checks': len(context.checks),
                      'report': str(out / 'report.json'), 'error': report.get('error')}))
    return 0 if report['status'] == 'passed' else 1

