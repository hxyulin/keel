#!/usr/bin/env python3
"""Real rustc/clang++ assembly through `keel asm` versus the compiler's own assembler.

Each case compiles one fixture to assembly with an installed compiler, then
assembles that unchanged `.s` with Clang's integrated assembler (reference) and
with Keel. Accepted outputs are compared structurally (sections, bytes,
relocations, symbols, DWARF dumps) and, for AArch64 Mach-O, both objects are
linked identically and executed as a runtime oracle. Assembly success alone is
never counted as correctness.

Every case runs as one child under the shared lock:
    python3 suite/fast_opt_locked.py LOG python3 run.py --one CASE.json
Example:
    python3 suite/compiler_asm_compat/run.py \
        --keel target/debug/keel --out target/compiler-asm-compat \
        --no-timing --chain-limit 0
Rerun aggregation only: add --report-only.

Rejected inputs keep the raw stderr, the first unsupported source line, and an
optional diagnostic error chain built from a scratch copy with failing lines
blanked. Chain objects are never compared or executed; they only rank work.
"""
import argparse
import hashlib
import json
import os
import re
import shlex
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import objinspect  # noqa: E402

ROOT = HERE.parent.parent
LOCK = ROOT / 'suite' / 'fast_opt_locked.py'
A64 = 'arm64-apple-macosx11.0.0'
X86 = 'x86_64-unknown-linux-gnu'

# Feature tags decide the report group: core (supported subset) vs EH/TLS/debug-rich.
FIXTURES = {
    'cpp/templates': ['templates', 'constexpr', 'lambdas'],
    'cpp/virtual': ['classes', 'inheritance', 'virtual-dispatch', 'rtti'],
    'cpp/static_init': ['static-init', 'guards', 'atexit'],
    'cpp/atomics': ['atomics'],
    'cpp/vector_loops': ['vector-loops'],
    'cpp/raii_eh': ['eh', 'raii'],
    'cpp/tls': ['tls', 'threads'],
    'rust/extern_c': ['extern-c', 'scalars', 'slices', 'enums'],
    'rust/std_generic': ['std', 'generics', 'trait-objects'],
    'rust/panic_unwind': ['eh', 'panic-unwind'],
    'rust/tls_atomics': ['tls', 'threads', 'atomics'],
}
OPTS = ['O0', 'O2']
DEBUG = [False, True]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def group_of(case):
    tags = FIXTURES[case['fixture']]
    if 'eh' in tags:
        return 'eh'
    if 'tls' in tags:
        return 'tls'
    if case['debug']:
        return 'debug'
    return 'core'


def cases():
    out = []
    for fixture in FIXTURES:
        lang, name = fixture.split('/')
        for opt in OPTS:
            for debug in DEBUG:
                suffix = f'{opt}' + ('-g5' if debug else '')
                out.append(dict(id=f'{lang}-{name}-a64-{suffix}', fixture=fixture, lang=lang, name=name,
                                target='a64', dialect='default', opt=opt, debug=debug))
                for dialect in ('att', 'intel'):
                    out.append(dict(id=f'{lang}-{name}-x86{dialect}-{suffix}', fixture=fixture, lang=lang, name=name,
                                    target='x86', dialect=dialect, opt=opt, debug=debug))
    for c in out:
        c['group'] = group_of(c)
        c['tags'] = FIXTURES[c['fixture']]
    return out


# ---------------------------------------------------------------- child side

def run(cmd, log, timeout=600, cwd=None, env=None):
    started = time.time()
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=timeout, cwd=cwd, env=env)
        rc, stdout, stderr = r.returncode, r.stdout, r.stderr
    except subprocess.TimeoutExpired as e:
        rc, stdout, stderr = 'timeout', e.stdout or b'', e.stderr or b''
    rec = dict(cmd=[str(c) for c in cmd], rc=rc, wall_s=round(time.time() - started, 4),
               stdout=stdout.decode(errors='replace'), stderr=stderr.decode(errors='replace'))
    log.append(rec)
    return rec


def generate(case, d, log):
    """Compile the fixture to assembly. Returns (asm path, extra dict) or (None, error)."""
    src = HERE / 'fixtures' / case['lang'] / (case['name'] + ('.cpp' if case['lang'] == 'cpp' else '.rs'))
    extra = {'source': str(src.relative_to(ROOT)), 'source_sha256': sha(src)}
    asm = d / f'{case["name"]}.s'
    if case['lang'] == 'cpp':
        cmd = ['clang++', '-std=c++20', '-target', A64 if case['target'] == 'a64' else X86, '-S',
               f'-{case["opt"]}']
        if case['debug']:
            cmd += ['-g', '-gdwarf-5']
        if case['dialect'] == 'intel':
            cmd += ['-masm=intel']
        cmd += [str(src), '-o', str(asm)]
        r = run(cmd, log)
        extra['generate'] = cmd
        return (asm if r['rc'] == 0 else None), extra
    level = '0' if case['opt'] == 'O0' else '2'
    cmd = ['rustc', '--edition', '2021', '--crate-name', case['name'], '-C', f'opt-level={level}',
           '-C', 'codegen-units=1']
    if case['debug']:
        cmd += ['-C', 'debuginfo=2', '-C', 'dwarf-version=5']
    if case['target'] == 'a64':
        # save-temps keeps the per-CGU objects and symbols.o referenced by the printed link line.
        cmd += ['-C', 'save-temps', '--emit=asm,obj,link', '--print', 'link-args', '-o', str(d / case['name']),
                str(src)]
        r = run(cmd, log)
        extra['generate'] = cmd
        if r['rc'] != 0:
            return None, extra
        cgu = sorted(d.glob(f'{case["name"]}.{case["name"]}.*-cgu.0.rcgu.s'))
        extra['rust_link_args'] = r['stdout'].strip()
        extra['rustc_object'] = str(d / f'{case["name"]}.o')
        extra['rustc_cgu_asm'] = [str(p) for p in cgu]
        others = [str(p) for p in d.glob('*.rcgu.o') if '-cgu.0.' not in p.name]
        extra['rustc_other_objects'] = others  # allocator shim: rustc-built, shared by both arms
        return asm, extra
    cmd += ['--target', 'x86_64-unknown-linux-gnu', '--emit', f'asm={asm}', '--emit', f'obj={d / case["name"]}.rustc.o']
    if case['dialect'] == 'intel':
        cmd += ['-C', 'llvm-args=-x86-asm-syntax=intel']
    cmd += [str(src)]
    r = run(cmd, log)
    extra['generate'] = cmd
    extra['rustc_object'] = str(d / f'{case["name"]}.rustc.o')
    return (asm if r['rc'] == 0 else None), extra


def keel_cmd(keel, case, asm, out, cpu='apple-m5'):
    if case['target'] == 'a64':
        return [keel, 'asm', '--cpu', cpu, str(asm), '-o', str(out)]
    return [keel, 'asm', '--target', X86, '--x86-syntax', case['dialect'], str(asm), '-o', str(out)]


def ref_cmd(case, asm, out):
    return ['clang', '-c', '-target', A64 if case['target'] == 'a64' else X86, str(asm), '-o', str(out)]


ERR_LINE = re.compile(r'line (\d+): (.*)')


def first_error(stderr, asm_lines):
    """Keel reports `keel: PATH: line N: message`; Clang reports `PATH:N:C: error: message`."""
    text = stderr.strip().splitlines()
    for line in text:
        m = ERR_LINE.search(line)
        if m:
            n = int(m.group(1))
            src = asm_lines[n - 1].strip() if 0 < n <= len(asm_lines) else ''
            return dict(line=n, message=m.group(2).strip(), source=src, category=categorize(m.group(2), src))
        m = re.search(r':(\d+):\d+: error: (.*)', line)
        if m:
            n = int(m.group(1))
            src = asm_lines[n - 1].strip() if 0 < n <= len(asm_lines) else ''
            return dict(line=n, message=m.group(2).strip(), source=src, category=categorize(m.group(2), src))
    return dict(line=None, message=text[-1] if text else '(no stderr)', source='', category='other: ' + (text[-1][:80] if text else 'no stderr'))


def categorize(message, source):
    """Stable feature key: directive or mnemonic shape, not the operand values."""
    tok = source.split()[0] if source.split() else ''
    m = re.match(r'unsupported directive `([^`]+)`', message)
    if m:
        return f'directive {m.group(1)}'
    m = re.match(r'unsupported x86 instruction `([^`]+)`', message)
    if m:
        return f'x86 instruction {m.group(1)}'
    if message.startswith('invalid CFI register'):
        return f'CFI register operand ({tok} {source.split()[1].rstrip(",") if len(source.split()) > 1 else ""})'
    if 'Apple SIMD mnemonic suffix' in message:
        return f'Apple SIMD suffix syntax ({tok.split(".")[-1]})'
    generic = re.sub(r'`[^`]*`', '`*`', message)
    generic = re.sub(r'\b0x[0-9a-f]+\b|\b\d+\b', 'N', generic)
    return f'{generic} [{tok}]'


def error_chain(keel, case, asm, d, log, limit):
    """Diagnostic only: blank each failing feature class in a scratch copy, collect next errors."""
    lines = asm.read_text(errors='replace').splitlines()
    scratch = d / 'chain-scratch.s'
    out, chain, blanked = d / 'chain-scratch.o', [], set()
    for _ in range(limit):
        scratch.write_text('\n'.join(lines) + '\n')
        r = subprocess.run(keel_cmd(keel, case, scratch, out), capture_output=True, text=True)
        if r.returncode == 0:
            chain.append({'end': 'accepted after blanking', 'blanked_lines': len(blanked)})
            break
        err = first_error(r.stderr, lines)
        if err['line'] is None or err['line'] in blanked:
            chain.append({'end': 'stopped', 'raw': r.stderr.strip()[:300]})
            break
        same = chain_key(err, lines[err['line'] - 1])
        hits = [i for i, text in enumerate(lines) if text.strip() and same(text)]
        chain.append({**{k: err[k] for k in ('line', 'message', 'source', 'category')}, 'blanked': len(hits)})
        for i in hits:
            lines[i] = ''
            blanked.add(i + 1)
    else:
        chain.append({'end': f'limit {limit}'})
    for p in (scratch, out):
        p.unlink(missing_ok=True)
    return chain


def chain_key(err, failing):
    """Which lines share the failing feature, so one chain step = one feature (diagnostic only)."""
    words = failing.split(';')[0].split()
    head = words[0] if words else ''
    msg = err['message']
    if head in ('.file', '.loc') or 'MD5' in msg or '.loc' in msg or '.file' in msg:
        # A rejected .file cascades into every .loc; drop the whole line-table directive set at once.
        return lambda t: t.split()[0] in ('.file', '.loc')
    if msg.startswith('invalid CFI register'):
        return lambda t: t.split()[:2] == words[:2]
    if head.startswith('.') and not head.startswith('.section') and not msg.startswith(('expected', 'assignments')):
        return lambda t: t.split()[0] == head
    if not head.startswith('.') and not head.endswith(':'):
        return lambda t: t.split()[0] == head  # mnemonic, including Apple `.4s` suffix forms
    return lambda t: t.strip() == failing.strip()


CENSUS_PATTERNS = {
    'loh': r'^\s*\.loh\b',
    'cfi': r'^\s*\.cfi_',
    'cfi_personality/lsda': r'^\s*\.cfi_(personality|lsda)\b',
    'cfi_remember/restore_state': r'^\s*\.cfi_(remember|restore)_state\b',
    'uleb128/sleb128': r'^\s*\.[us]leb128\b',
    'weak_def_can_be_hidden': r'^\s*\.weak_def_can_be_hidden\b',
    'weak_reference/weak_definition': r'^\s*\.weak_(reference|definition)\b',
    'private_extern/hidden': r'^\s*\.(private_extern|hidden)\b',
    'tbss/tlv (Mach-O TLS)': r'^\s*\.(tbss|tlv)|__thread_(vars|data|bss|ptr)|@TLVPPAGE',
    'ELF TLS relocs': r'@(TPOFF|GOTTPOFF|TLSGD|TLSLD|DTPOFF|tpoff|gottpoff|tlsgd|tlsld|dtpoff)\b',
    'mod_init_func': r'__mod_init_func|\.init_array',
    'gcc_except_table': r'gcc_except_tab',
    'GOT page/load': r'@GOTPAGE|@GOTPAGEOFF|@GOTPCREL|GOTPCREL',
    'PLT': r'@PLT\b',
    'Apple SIMD suffix syntax': r'^\s*[a-z0-9]+\.(\d+)?[bhsdq]\s',
    'SIMD lane/vector regs': r'\bv\d+\.\d*[bhsd]\b|\bv\d+\[\d+\]|\b[xy]mm\d+\b',
    'LSE atomics': r'^\s*(cas|ldadd|ldclr|ldset|ldeor|swp)[a-z]*\s',
    'exclusive atomics': r'^\s*(ldaxr|stlxr|ldxr|stxr)[bh]?\s',
    'x86 lock prefix': r'^\s*lock\b',
    'DWARF .file/.loc': r'^\s*\.(file|loc)\b',
    'DWARF sections': r'__DWARF|\.debug_',
    'set/symbol arithmetic': r'^\s*\.set\b|-\s*L|-\s*\.L',
    'alt_entry': r'^\s*\.alt_entry\b',
    'zerofill/comm': r'^\s*\.(zerofill|comm|lcomm)\b',
    'section comdat groups': r'\.section\s+[^\n]*,comdat',
    'prefalign': r'^\s*\.prefalign\b',
    'AT&T operands (%reg)': r'^\s*[a-z][a-z0-9]*\s[^;#\n]*%(r[a-z0-9]+|e[a-z]{2}|[xy]mm\d+)\b',
    'intel_syntax': r'^\s*\.intel_syntax',
    'addrsig': r'^\s*\.addrsig',
    'ident': r'^\s*\.ident',
    'build_version': r'^\s*\.build_version',
    'type/size (ELF)': r'^\s*\.(type|size)\b',
}


def census(asm_text):
    out = {}
    for key, pat in CENSUS_PATTERNS.items():
        n = len(re.findall(pat, asm_text, re.M))
        if n:
            out[key] = n
    directives = {}
    for m in re.finditer(r'^\s*(\.[A-Za-z_][\w.]*)', asm_text, re.M):
        directives[m.group(1)] = directives.get(m.group(1), 0) + 1
    return out, directives


def icount(cmd, runs=3):
    """Retired instructions/wall for an accepted command (macOS /usr/bin/time -l)."""
    samples = []
    for _ in range(runs):
        t0 = time.time()
        r = subprocess.run(['/usr/bin/time', '-l', *map(str, cmd)], capture_output=True, text=True)
        wall = time.time() - t0
        m = re.search(r'(\d+)\s+instructions retired', r.stderr)
        if r.returncode or not m:
            return None
        samples.append((int(m.group(1)), wall))
    return dict(instructions_median=statistics.median(s[0] for s in samples),
                wall_s_median=round(statistics.median(s[1] for s in samples), 4), runs=runs)


def dwarf_dump(obj, d, tag, log):
    r = run(['dwarfdump', '--debug-info', '--debug-line', '--debug-str', '--debug-abbrev', str(obj)], log)
    text = '\n'.join(l for l in r['stdout'].splitlines() if str(obj) not in l and 'file format' not in l)
    (d / f'dwarf-{tag}.txt').write_text(text)
    v = run(['dwarfdump', '--verify', str(obj)], log)
    return text, dict(dump_rc=r['rc'], verify_rc=v['rc'],
                      verify_tail=(v['stdout'] + v['stderr']).strip().splitlines()[-2:])


def link_and_run(case, extra, obj, d, tag, log):
    exe = d / f'exe-{tag}'
    if case['lang'] == 'cpp':
        cmd = ['clang++', '-target', A64, str(obj), '-o', str(exe)]
    else:
        args = shlex.split(extra['rust_link_args'])
        swapped = 0
        for i, a in enumerate(args):
            if a.endswith('-cgu.0.rcgu.o'):
                args[i] = str(obj)
                swapped += 1
            if a == '-o':
                args[i + 1] = str(exe)
        if swapped != 1:
            return dict(link_rc='no-cgu0-in-link-args')
        cmd = args
    r = run(cmd, log)
    if r['rc'] != 0:
        return dict(link_rc=r['rc'], link_stderr=r['stderr'][-2000:])
    e = run([str(exe)], log, timeout=60)
    return dict(link_rc=0, exe=str(exe), rc=e['rc'], stdout=e['stdout'], stderr=e['stderr'])


def one(case_file):
    case = json.loads(Path(case_file).read_text())
    d = Path(case['dir'])
    keel = case['keel']
    log = []
    res = dict(case=case, versions=case['versions'])
    asm, extra = generate(case, d, log)
    res.update(extra)
    res['generated'] = asm is not None and asm.exists()
    if not res['generated']:
        res['commands'] = log
        return res
    asm_text = asm.read_text(errors='replace')
    asm_lines = asm_text.splitlines()
    res['asm'] = dict(path=str(asm), sha256=sha(asm), bytes=asm.stat().st_size, lines=len(asm_lines))
    res['census'], res['directives'] = census(asm_text)
    ref_obj, keel_obj = d / 'ref.o', d / 'keel.o'
    r = run(ref_cmd(case, asm, ref_obj), log)
    res['ref'] = dict(kind='clang-integrated-as', rc=r['rc'], stderr=r['stderr'])
    if r['rc'] != 0:
        res['ref']['first_error'] = first_error(r['stderr'], asm_lines)
        # Explicit LLVM-version-skew fallback, not a general waiver of Clang failures.
        # This object comes from the same compilation, not from assembling the .s.
        if (case['lang'] == 'rust' and case['target'] == 'x86'
                and res['ref']['first_error']['source'].startswith('.prefalign')
                and 'unknown directive' in res['ref']['first_error']['message']
                and Path(res.get('rustc_object', '')).is_file()):
            shutil.copy(res['rustc_object'], ref_obj)
            res['ref']['fallback'] = 'rustc --emit obj (same LLVM module as the .s)'
    if ref_obj.exists():
        res['ref']['sha256'] = sha(ref_obj)
    res['asm']['sha256_before_keel'] = sha(asm)
    k = run(keel_cmd(keel, case, asm, keel_obj), log)
    res['keel'] = dict(rc=k['rc'], stderr=k['stderr'])
    if k['rc'] != 0:
        res['keel']['first_error'] = first_error(k['stderr'], asm_lines)
        if case['chain_limit']:
            res['keel']['chain'] = error_chain(keel, case, asm, d, log, case['chain_limit'])
    else:
        res['keel']['sha256'] = sha(keel_obj)
    if case['target'] == 'a64':
        b = run(keel_cmd(keel, case, asm, d / 'keel-baseline.o', cpu='baseline'), log)
        res['keel_baseline'] = dict(rc=b['rc'])
        if b['rc'] != 0:
            res['keel_baseline']['first_error'] = first_error(b['stderr'], asm_lines)
    if ref_obj.exists() and keel_obj.exists() and k['rc'] == 0:
        res['byte_identical'] = sha(ref_obj) == sha(keel_obj)
        try:
            res['structure'] = objinspect.compare(objinspect.parse(ref_obj), objinspect.parse(keel_obj))
        except Exception as exc:  # parse failure is itself a finding
            res['structure'] = dict(error=repr(exc))
        if case['debug']:
            ta, va = dwarf_dump(ref_obj, d, 'ref', log)
            tb, vb = dwarf_dump(keel_obj, d, 'keel', log)
            mask = lambda t: re.sub(r'(total_length|unit_length|length): 0x[0-9a-f]+', r'\1: <len>', t)
            dumps_ok = va['dump_rc'] == vb['dump_rc'] == 0
            res['dwarf'] = dict(dump_equal=dumps_ok and ta == tb,
                                dump_equal_modulo_lengths=dumps_ok and mask(ta) == mask(tb), ref=va, keel=vb)
        if res['ref']['rc'] == 0 and case['timing']:
            res['timing'] = dict(clang=icount(ref_cmd(case, asm, d / 'timing-ref.o')),
                                 keel=icount(keel_cmd(keel, case, asm, d / 'timing-keel.o')))
    if case['target'] == 'a64' and ref_obj.exists():
        rt = dict(ref=link_and_run(case, res, ref_obj, d, 'ref', log))
        if case['lang'] == 'rust':
            rt['rustc'] = dict(rc=None)
            e = run([str(d / case['name'])], log, timeout=60)
            rt['rustc'] = dict(rc=e['rc'], stdout=e['stdout'], stderr=e['stderr'])
        if keel_obj.exists() and k['rc'] == 0:
            rt['keel'] = link_and_run(case, res, keel_obj, d, 'keel', log)
            a, b = rt['ref'], rt['keel']
            rt['match'] = a.get('link_rc') == 0 and b.get('link_rc') == 0 and \
                (a['rc'], a['stdout'], a['stderr']) == (b['rc'], b['stdout'], b['stderr'])
            if case['debug'] and a.get('link_rc') == b.get('link_rc') == 0:
                for tag in ('ref', 'keel'):
                    exe = Path(rt[tag]['exe'])
                    ds = run(['dsymutil', str(exe), '-o', str(exe) + '.dSYM'], log)
                    vr = run(['dwarfdump', '--verify', str(exe) + '.dSYM'], log)
                    rt[tag]['dsymutil_rc'] = ds['rc']
                    rt[tag]['dsymutil_warnings'] = len([l for l in ds['stderr'].splitlines() if 'warning' in l])
                    rt[tag]['dsym_verify_rc'] = vr['rc']
        rt['ref_ok'] = rt['ref'].get('link_rc') == 0 and rt['ref'].get('rc') == 0
        if 'rustc' in rt:
            a, b = rt['ref'], rt['rustc']
            rt['rustc_matches_ref'] = a.get('link_rc') == 0 and b.get('rc') == 0 and all(
                a.get(key) == b.get(key) for key in ('rc', 'stdout', 'stderr'))
            rt['ref_ok'] = rt['ref_ok'] and rt['rustc_matches_ref']
        rt['match'] = rt['ref_ok'] and rt.get('match', False)
        res['runtime'] = rt
    res['asm']['sha256_after'] = sha(asm)
    res['asm']['unchanged'] = res['asm']['sha256'] == res['asm']['sha256_before_keel'] == res['asm']['sha256_after']
    src = ROOT / res['source']
    res['source_unchanged'] = sha(src) == res['source_sha256']
    res['commands'] = log
    return res


# ---------------------------------------------------------------- parent side

def versions(keel, lock_log=None):
    v = {}
    for name, cmd in [('rustc', ['rustc', '-vV']), ('clang++', ['clang++', '--version']),
                      ('clang', ['clang', '--version']), ('ld', ['ld', '-v']), ('dwarfdump', ['dwarfdump', '--version']),
                      ('sw_vers', ['sw_vers'])]:
        if lock_log is not None:
            cmd = [sys.executable, str(LOCK), str(lock_log), *cmd]
        r = subprocess.run(cmd, capture_output=True, text=True)
        v[name] = (r.stdout + r.stderr).strip()
    v['keel'] = dict(path=str(keel), sha256=sha(keel))
    v['keel_repo_head'] = subprocess.run(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], capture_output=True,
                                         text=True).stdout.strip()
    return v


def metadata_status(r):
    """Debug-tool evidence is independent of object equality and runtime success."""
    if not r['case']['debug']:
        return 'not-requested'
    dwarf = r.get('dwarf')
    if not dwarf:
        return 'unchecked'
    checks = [dwarf.get(tag, {}).get(key) for tag in ('ref', 'keel') for key in ('dump_rc', 'verify_rc')]
    if r['case']['target'] == 'a64':
        rt = r.get('runtime', {})
        checks += [rt.get(tag, {}).get(key) for tag in ('ref', 'keel')
                   for key in ('dsymutil_rc', 'dsym_verify_rc')]
    if any(rc is not None and rc != 0 for rc in checks):
        return 'failed'
    if any(rc is None for rc in checks):
        return 'unchecked'
    if not dwarf.get('dump_equal') or any(
            r.get('runtime', {}).get(tag, {}).get('dsymutil_warnings', 0) for tag in ('ref', 'keel')):
        return 'differs'
    return 'equal'


def verdict(r):
    """Label object evidence, with separate runtime and debug-tool qualifications."""
    if not r.get('generated'):
        return 'generate-failed'
    if 'recompare_hashes' in r and not all(r['recompare_hashes'].values()):
        return 'artifact-hash-mismatch'
    if r.get('asm', {}).get('unchanged') is False or r.get('source_unchanged') is False:
        return 'source-changed'
    if r['keel']['rc'] != 0:
        return 'keel-rejected'
    if r['ref']['rc'] != 0 and 'fallback' not in r['ref']:
        return 'reference-rejected'
    rt = r.get('runtime')
    s = r.get('structure', {})
    if rt is not None:
        if not rt.get('ref_ok'):
            return 'reference-runtime-failed'
        if not rt.get('match'):
            return 'RUNTIME-MISMATCH'
    if 'error' in s:
        return 'structure-parse-error'
    suffix = '+runtime' if rt else ('+runtime-unchecked' if r['case']['target'] == 'a64' else '')
    if not r.get('asm', {}).get('unchanged') or not r.get('source_unchanged'):
        suffix += '+hash-unchecked'
    meta = metadata_status(r)
    if meta not in ('not-requested', 'equal'):
        suffix += '+metadata-' + meta
    if r.get('byte_identical'):
        return 'byte-identical' + suffix
    if s.get('all_equal'):
        return 'structure-equal' + suffix
    if s.get('all_equal_benign'):
        return 'structure-equal-modulo-benign' + suffix
    if s.get('code_data_equal'):
        return 'code-data-equal(meta-differs)' + suffix
    return ('structure-differs' if s else 'accepted-uncompared') + suffix


# Chain categories -> candidate fix units. `cascade` marks errors produced only because an earlier
# diagnostic blanking moved data (e.g. a dropped `.section` line spills DWARF bytes into __bss).
FIX_GROUPS = [
    ('cascade', r'^\.loc references undeclared file|^zero-fill sections|^relocations require a file-backed'),
    ('a64: .weak_def_can_be_hidden/.weak_definition (Mach-O weak defs)', r'^directive \.weak_(def_can_be_hidden|definition)'),
    ('a64: .cfi_def_cfa wsp (CFI register alias)', r'^CFI register operand'),
    ('a64: DWARF5 .file with MD5 on N>0 but not on root .file 0', r'MD5 checksums'),
    ('a64: __DWARF,__debug_aranges section', r'__debug_aranges'),
    ('a64: Apple SIMD suffix forms (cnt/uaddlv .8b, lane-indexed fmla/fmul.4s, smlal(2), fcmgt/fcmlt #0.0)',
     r'^Apple SIMD suffix|^Apple suffix requires'),
    ('a64: static init sections (__TEXT,__StaticInit; __DATA,__mod_init_func)', r'__StaticInit|__mod_init_func'),
    ('a64: C++ TLS (.tbss without alignment; __ZTH alias = ___tls_init)', r'\[\.tbss\]|__ZTH'),
    ('a64: LRCPC2 ldapur/stlur', r'\[(ldapur|stlur)\]'),
    ('x86: .file forms (`.file "name"`, DWARF5 .file 0/dir/md5, .loc operands)', r'\[\.file\]|\[\.loc\]|x86 \.file|\.file 0|\.file N'),
    ('x86: .ident/.addrsig/.addrsig_sym', r'`\*` \[\.(ident|addrsig|addrsig_sym)\]'),
    ('x86: rustc .prefalign', r'\[\.prefalign\]'),
    ('x86: CFI EH directives (.cfi_personality/.cfi_lsda/.cfi_adjust_cfa_offset)', r'\[\.cfi_'),
    ('x86: lock prefix', r'^x86 instruction lock'),
    ('x86: shift-by-1 short form (shl/shr/sar reg)', r'for (Shl|Shr|Sar)'),
    ('x86: SSE/SSE2 integer+float vector instructions', r'^x86 instruction (p|movdq|shufps|unpck|minps|maxps|cmp\w+(ps|ss)|andn?ps|cvt)'),
    ('x86: .uleb128 label-difference expressions', r'\[\.uleb128\]'),
    ('x86: string escapes (\\b etc.) in .ascii/.asciz', r'string escape'),
    ('x86: .section forms (@init_array, unique,N, M without group)', r'unsupported section type|unexpected section arguments'),
]


def fix_group(category):
    for name, pat in FIX_GROUPS:
        if re.search(pat, category):
            return name
    return 'other: ' + category


def greedy_unblock(results, target):
    """Greedy set cover over diagnostic chains: which fix units make the most cases assemble."""
    needs = {}
    for r in results:
        c = r['case']
        key = c['target'] + ('' if c['target'] == 'a64' else '-' + c['dialect'])
        if key != target or not r.get('generated') or r['keel']['rc'] == 0:
            continue
        chain = r['keel'].get('chain') or []
        if not chain or 'accepted' not in str(chain[-1].get('end', '')):
            needs[c['id']] = {'(chain incomplete)'}
            continue
        needs[c['id']] = {fix_group(s['category'] + ' ' + s['source']) for s in chain if 'category' in s} - {'cascade'}
    picked, steps = set(), []
    while True:
        open_cases = {i: n - picked for i, n in needs.items() if n - picked}
        if not open_cases:
            break
        cands = {g for n in open_cases.values() for g in n}
        def score(g):
            done = sum(1 for n in open_cases.values() if n <= picked | {g})
            return (done, sum(1 for n in open_cases.values() if g in n))
        best = max(sorted(cands), key=score)
        picked.add(best)
        done = [i for i, n in needs.items() if n <= picked]
        steps.append((best, score(best)[1], len(done)))
        if len(steps) > 30:
            break
    return steps, len(needs)


def report(out, results, vers, args):
    L = []
    w = L.append
    w('Keel assembler compatibility with real rustc / clang++ assembly output')
    w(f'Generated {time.strftime("%Y-%m-%d %H:%M:%S %Z")} by suite/compiler_asm_compat/run.py')
    w('')
    w('== Tools ==')
    w(f'keel: {vers["keel"]["path"]} sha256={vers["keel"]["sha256"]}')
    w(f'harness repo HEAD at run: {vers["keel_repo_head"]} (binary provenance is its path/hash; HEAD does not identify its build)')
    for name in ('rustc', 'clang++', 'ld', 'dwarfdump', 'sw_vers'):
        w(f'{name}: ' + ' | '.join(l.strip() for l in vers[name].splitlines()[:6]))
    w('')
    w('== Method ==')
    w('Per case: compiler emits .s (unchanged) -> reference = `clang -c -target T x.s`; candidate = `keel asm`.')
    w('AArch64 Mach-O: keel --cpu apple-m5 (also --cpu baseline for feature gating); both objects are linked with')
    w('the identical link line (clang++ for C++; rustc --print link-args with only the cgu.0 object swapped for')
    w('Rust; the rustc-built allocator-shim/symbols.o objects are shared by both arms) and executed; stdout,')
    w('stderr and exit code must match the clang-assembled binary, which must itself exit 0.')
    w('x86_64-unknown-linux-gnu ELF: structure only (no Linux runtime). Compiler-default AT&T and Intel dialect')
    w('(clang++ -masm=intel / rustc -C llvm-args=-x86-asm-syntax=intel) are separate cases.')
    w('Keel uses explicit --x86-syntax att/intel. Assembly SHA-256 is checked before Keel and after all tools.')
    w('Apple Clang 21 may reject Rust .prefalign: only that failure permits the explicitly marked rustc')
    w('--emit obj fallback from the same compilation. It is not an assembly-derived reference.')
    w('Structure = per-section bytes/size/type/flags/alignment, order-insensitive normalized relocations')
    w('(local targets resolved to section+offset), full symbol table, build_version, LOH. -g5 cases also diff')
    w('`dwarfdump --debug-info/--debug-line/--debug-str/--debug-abbrev` text, `dwarfdump --verify`, and run')
    w('dsymutil + verify on the linked binaries. Assembly success alone is never counted as correct.')
    w('Chains: diagnostic-only scratch copies; each step blanks the failing feature class (same directive, same')
    w('mnemonic, same CFI form, or all .file/.loc) to expose the next blocker; never compared/linked/run.')
    w('No chain result is compatibility evidence. Full diagnostics are retained in commands, without truncation.')
    w('Strict structure requires equal alignment and entry sizes; the benign Mach-O tier permits only ltmp')
    w('locals and relocation attribute bits. Metadata tool failures/differences qualify verdicts independently.')
    w('ELF .llvm_addrsig indexes resolve to symbol records; .comment/.note.GNU-stack/.llvm_addrsig are metadata.')
    w('Runtime match does not imply object equality; normalized structure does not imply complete object equivalence.')
    w('')
    total = len(results)
    gen = sum(r.get('generated', False) for r in results)
    refok = sum(r.get('generated', False) and r['ref']['rc'] == 0 for r in results)
    reffb = sum(r.get('generated', False) and 'fallback' in r['ref'] for r in results)
    kok = [r for r in results if r.get('generated') and r['keel']['rc'] == 0]
    w('== Validation counts ==')
    w(f'cases={total} generated={gen} clang-ref-assembled={refok} rustc-obj-fallback-ref={reffb} keel-accepted={len(kok)}')
    verdicts = {}
    for r in results:
        verdicts.setdefault(verdict(r), []).append(r['case']['id'])
    for v, ids in sorted(verdicts.items(), key=lambda kv: -len(kv[1])):
        w(f'  {v:40s} {len(ids)}')
    w('  debug metadata: ' + ', '.join(f'{status}={sum(metadata_status(r) == status for r in results)}'
                                    for status in ('equal', 'differs', 'failed', 'unchecked', 'not-requested')))
    w('')
    w('-- by target/dialect x group (accepted / runtime-match / structure-equal-modulo-benign / byte-identical / total) --')
    keys = sorted({(r['case']['target'] + ('' if r['case']['target'] == 'a64' else '-' + r['case']['dialect']),
                    r['case']['group']) for r in results})
    for key in keys:
        rs = [r for r in results if (r['case']['target'] + ('' if r['case']['target'] == 'a64' else '-' + r['case']['dialect']),
                                     r['case']['group']) == key]
        acc = [r for r in rs if r.get('generated') and r['keel']['rc'] == 0]
        rtm = sum(bool(r.get('runtime', {}).get('ref_ok') and r.get('runtime', {}).get('match')) for r in acc)
        st = sum(bool(r.get('structure', {}).get('all_equal_benign')) for r in acc)
        bi = sum(bool(r.get('byte_identical')) for r in acc)
        w(f'  {key[0]:10s} {key[1]:6s} {len(acc):3d} / {rtm:3d} / {st:3d} / {bi:3d} / {len(rs):3d}')
    w('  (groups: core = supported-subset candidates; eh/tls = fixtures needing EH or TLS; debug = core fixture + -g5)')
    w('')
    w('== Per case ==')
    w(f'{"case":40s} {"keel":6s} {"m5/base":8s} {"verdict":34s} first unsupported feature (line: source)')
    for r in results:
        c = r['case']
        if not r.get('generated'):
            w(f'{c["id"]:40s} {"-":6s} {"-":8s} {"generate-failed":34s}')
            continue
        k = r['keel']
        base = r.get('keel_baseline', {}).get('rc', '-')
        fe = k.get('first_error')
        note = f'{fe["category"]} (L{fe["line"]}: {fe["source"][:70]})' if fe else ''
        if not fe and r.get('keel_baseline', {}).get('first_error'):
            bf = r['keel_baseline']['first_error']
            note = f'[baseline cpu only] {bf["category"]} (L{bf["line"]}: {bf["source"][:60]})'
        w(f'{c["id"]:40s} {str(k["rc"]):6s} {str(k["rc"])+"/"+str(base):8s} {verdict(r):34s} {note}')
    w('')
    w('== First-error categories (cases blocked) ==')
    firsts = {}
    for r in results:
        fe = r.get('keel', {}).get('first_error')
        if fe:
            firsts.setdefault(fe['category'], []).append(r['case']['id'])
    for cat, ids in sorted(firsts.items(), key=lambda kv: -len(kv[1])):
        w(f'  {len(ids):3d}  {cat}')
        w(f'       e.g. {", ".join(ids[:4])}{" ..." if len(ids) > 4 else ""}')
    w('')
    w('== Diagnostic chains: categories seen anywhere in a blocked case (cases containing) ==')
    chain_cats = {}
    ends = {}
    for r in results:
        ch = r.get('keel', {}).get('chain') or []
        for step in ch:
            if 'category' in step:
                chain_cats.setdefault(step['category'], set()).add(r['case']['id'])
            elif 'end' in step:
                ends.setdefault(step['end'].split(' after')[0], []).append(r['case']['id'])
    for cat, ids in sorted(chain_cats.items(), key=lambda kv: -len(kv[1])):
        tg = sorted({i.split('-')[2] for i in ids})
        w(f'  {len(ids):3d}  {cat}   [{",".join(tg)}]')
    for end, ids in ends.items():
        w(f'  chain end "{end}": {len(ids)} cases')
    w('')
    w('== Greedy unblock ranking from diagnostic chains (assembly acceptance only, not correctness) ==')
    w('  step. fix unit  [cases needing it among still-blocked] -> cumulative cases that would assemble')
    for target in ('a64', 'x86-intel', 'x86-att'):
        steps, n = greedy_unblock(results, target)
        w(f'  {target} ({n} blocked):')
        for i, (g, need, done) in enumerate(steps, 1):
            w(f'    {i:2d}. {g}  [{need}] -> {done}/{n}')
    w('')
    w('== Structure differences among keel-accepted cases ==')
    for r in kok:
        s = r.get('structure')
        if not s or s.get('all_equal'):
            continue
        fields = ('present', 'bytes', 'size', 'type_flags', 'align_equal', 'entsize', 'relocations')
        bad = [f'{n}[{v["class"]}]:' + ','.join(k + ('(benign-only)' if k == 'type_flags' and v.get('type_flags_benign') else '')
                                                 for k in fields if not v.get(k, False))
               for n, v in s.get('sections', {}).items() if not (v.get('present') and all(v.get(k) for k in fields))]
        extra = []
        if not s.get('symbols_equal', True):
            extra.append(f'symbols {s.get("symbol_counts")}' + (' (ltmp-only)' if s.get('symbols_equal_benign') else ''))
        if s.get('extra_sections'):
            extra.append(f'extra {s["extra_sections"]}')
        if not s.get('build_version_equal', True):
            extra.append('build_version')
        if not s.get('linker_hints_equal', True):
            extra.append('LOH')
        if not all(s.get(key, True) for key in ('format_equal', 'cputype_equal', 'header_equal')):
            extra.append('object format/CPU/header')
        w(f'  {r["case"]["id"]}: {"; ".join(bad[:6])} {" ".join(extra)}')
        for n, v in s.get('sections', {}).items():
            if v.get('reloc_examples'):
                w(f'      {n} relocs ref/keel {v["reloc_counts"]} ref_only={v["reloc_examples"]["ref_only"][:2]} '
                  f'keel_only={v["reloc_examples"]["keel_only"][:2]}')
        if s.get('symbol_examples'):
            w(f'      symbols ref_only={s["symbol_examples"]["ref_only"][:3]} keel_only={s["symbol_examples"]["keel_only"][:3]}')
        if 'dwarf' in r:
            w(f'      dwarf dump_equal={r["dwarf"]["dump_equal"]} modulo-lengths={r["dwarf"].get("dump_equal_modulo_lengths")} '
              f'verify ref/keel={r["dwarf"]["ref"]["verify_rc"]}/{r["dwarf"]["keel"]["verify_rc"]}')
    w('')
    w('== Debug metadata checks (independent of structure/runtime) ==')
    for r in results:
        if not r['case']['debug']:
            continue
        dwarf = r.get('dwarf', {})
        w(f'  {r["case"]["id"]}: {metadata_status(r)} dump_equal={dwarf.get("dump_equal")} '
          f'modulo-lengths(diagnostic only)={dwarf.get("dump_equal_modulo_lengths")} '
          f'dump rc ref/keel={dwarf.get("ref", {}).get("dump_rc")}/{dwarf.get("keel", {}).get("dump_rc")} '
          f'verify rc ref/keel={dwarf.get("ref", {}).get("verify_rc")}/{dwarf.get("keel", {}).get("verify_rc")}')
    w('')
    w('== Runtime oracle (AArch64 Mach-O) ==')
    for r in results:
        rt = r.get('runtime')
        if not rt:
            continue
        ref, kl = rt.get('ref', {}), rt.get('keel')
        line = f'  {r["case"]["id"]:40s} ref link={ref.get("link_rc")} rc={ref.get("rc")}'
        if 'rustc' in rt:
            line += f' rustc-bin rc={rt["rustc"]["rc"]} same-as-ref={rt.get("rustc_matches_ref")}'
        if kl:
            line += f' | keel link={kl.get("link_rc")} rc={kl.get("rc")} match={rt.get("match")}'
            if 'dsym_verify_rc' in kl:
                line += f' dsym verify ref/keel={ref.get("dsym_verify_rc")}/{kl.get("dsym_verify_rc")}'
        w(line)
    outs = {}
    for r in results:
        rt = r.get('runtime')
        if rt and rt.get('ref', {}).get('stdout'):
            outs.setdefault(r['case']['fixture'], set()).add(rt['ref']['stdout'].strip()[:160])
    w('  reference stdout by fixture (O0/O2/-g variants should agree):')
    for f, s in sorted(outs.items()):
        for o in sorted(s):
            w(f'    {f}: {o}')
    w('')
    w('== Feature census (cases whose .s contains feature: total / keel-accepted) ==')
    feat = {}
    for r in results:
        for key in r.get('census', {}):
            t = feat.setdefault(key, [0, 0])
            t[0] += 1
            t[1] += r['keel']['rc'] == 0
    for key, (n, a) in sorted(feat.items(), key=lambda kv: -kv[1][0]):
        w(f'  {key:32s} {n:3d} / {a:3d}')
    w('')
    timed = [r for r in kok if r.get('timing') and r['timing']['clang'] and r['timing']['keel']]
    if timed:
        w('== Optional timing (accepted outputs only; median of 3; /usr/bin/time -l retired instructions) ==')
        for r in timed:
            a, b = r['timing']['clang'], r['timing']['keel']
            w(f'  {r["case"]["id"]:40s} clang {a["instructions_median"]:>12,d} keel {b["instructions_median"]:>12,d}'
              f'  ratio {b["instructions_median"] / a["instructions_median"]:.3f}  asm {r["asm"]["bytes"]} B')
        w('')
    w('== Raw errors (one per distinct keel first-error category) ==')
    shown = set()
    for r in results:
        fe = r.get('keel', {}).get('first_error')
        if fe and fe['category'] not in shown:
            shown.add(fe['category'])
            cmd = next((c['cmd'] for c in r['commands'] if len(c['cmd']) > 1 and c['cmd'][1] == 'asm'), [])
            w(f'  [{fe["category"]}] case {r["case"]["id"]}')
            w(f'    $ {shlex.join(cmd)}')
            w(f'    {r["keel"]["stderr"].strip()[:400]}')
            w(f'    source L{fe["line"]}: {fe["source"]}')
    shown = set()
    for r in results:
        if r.get('generated') and r['ref']['rc'] != 0:
            fe = r['ref']['first_error']
            if fe['category'] not in shown:
                shown.add(fe['category'])
                w(f'  [reference clang rejected: {fe["category"]}] case {r["case"]["id"]}: {fe["message"]} '
                  f'L{fe["line"]}: {fe["source"]} (fallback: {r["ref"].get("fallback")})')
    w('')
    w('== Reproduce ==')
    w(f'  python3 suite/compiler_asm_compat/run.py --keel {args.keel} --out {args.out}')
    w('  per-case commands, stdout/stderr, rc, hashes: <out>/results.json and <out>/cases/<id>/result.json')
    notes = out / 'ANALYSIS.txt'  # hand-written review notes kept beside the artifacts
    if notes.exists():
        w('')
        w(notes.read_text().rstrip())
    (out / 'REPORT.txt').write_text('\n'.join(L) + '\n')


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--one', help=argparse.SUPPRESS)
    p.add_argument('--keel', type=Path, default=ROOT / 'target/debug/keel')
    p.add_argument('--out', type=Path, default=ROOT / 'target/compiler-asm-compat')
    p.add_argument('--only', default='', help='substring filter on case id')
    p.add_argument('--chain-limit', type=int, default=40)
    p.add_argument('--no-timing', action='store_true')
    p.add_argument('--lock-log', type=Path, help='shared lock log (default: OUT/lock.jsonl)')
    p.add_argument('--report-only', action='store_true')
    p.add_argument('--recompare', action='store_true',
                   help='with --report-only: re-parse on-disk ref.o/keel.o after an objinspect change (no tools run)')
    args = p.parse_args()
    if args.one:
        res = one(args.one)
        Path(json.loads(Path(args.one).read_text())['dir'], 'result.json').write_text(json.dumps(res, indent=1))
        return
    keel = args.keel.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    selected = [c for c in cases() if args.only in c['id']]
    vers_path = args.out / 'versions.json'
    if not args.report_only:
        lock_log = args.lock_log or args.out / 'lock.jsonl'
        vers = versions(keel, lock_log)
        vers_path.write_text(json.dumps(vers, indent=1))
        for i, c in enumerate(selected):
            d = (args.out / 'cases' / c['id']).resolve()
            if d.exists():
                shutil.rmtree(d)
            d.mkdir(parents=True)
            c.update(dir=str(d), keel=str(keel), versions=None, chain_limit=args.chain_limit,
                     timing=not args.no_timing)
            (d / 'case.json').write_text(json.dumps(c))
            r = subprocess.run([sys.executable, str(LOCK), str(lock_log), sys.executable, str(HERE / 'run.py'),
                                '--one', str(d / 'case.json')])
            res_path = d / 'result.json'
            status = verdict(json.loads(res_path.read_text())) if res_path.exists() else f'child rc={r.returncode}'
            print(f'[{i + 1}/{len(selected)}] {c["id"]}: {status}', flush=True)
    vers = json.loads(vers_path.read_text())
    results = []
    for c in selected:
        res_path = args.out / 'cases' / c['id'] / 'result.json'
        if res_path.exists():
            res = json.loads(res_path.read_text())
            d = res_path.parent
            if args.recompare and res.get('generated') and res['keel']['rc'] == 0 and (d / 'ref.o').exists():
                try:
                    artifacts = [(Path(res['asm']['path']), res['asm']['sha256']),
                                 (ROOT / res['source'], res['source_sha256']),
                                 (d / 'ref.o', res['ref'].get('sha256')),
                                 (d / 'keel.o', res['keel'].get('sha256'))]
                    res['recompare_hashes'] = {str(path): bool(expected) and sha(path) == expected
                                               for path, expected in artifacts}
                    if not all(res['recompare_hashes'].values()):
                        raise ValueError('artifact hashes changed or unavailable; refusing recompare')
                    res['structure'] = objinspect.compare(objinspect.parse(d / 'ref.o'), objinspect.parse(d / 'keel.o'))
                except Exception as exc:
                    res['structure'] = dict(error=repr(exc))
                res_path.write_text(json.dumps(res, indent=1))
            res['versions'] = None
            results.append(res)
    (args.out / 'results.json').write_text(json.dumps(dict(versions=vers, results=results), indent=1))
    report(args.out, results, vers, args)
    print(f'wrote {args.out / "REPORT.txt"}')


if __name__ == '__main__':
    main()
