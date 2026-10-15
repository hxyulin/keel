"""Generate deterministic large A64 inputs without compiling enormous C files.

Instruction-dense files stress encoding; function-heavy files add compiler-like
comments, labels, calls, CFI and subsection metadata. These are synthetic
scaling workloads, not complete applications. Files may exceed the requested
size by one complete generation unit.
"""
import argparse
import hashlib
import json
import pathlib

MIXED = ('add x0, x1, #17\nsub w3, w4, w5, lsl #2\n'
         'ldr x6, [x7, #24]\nldr d0, [x7, #16]\n'
         'fadd d0, d1, d2\nmovz x8, #1234\n'
         'and x9, x10, x11\nldaddal x0, x1, [x2]\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=pathlib.Path, required=True)
    parser.add_argument('--mib', nargs='+', type=int, default=[64, 256])
    args = parser.parse_args()
    if any(size <= 0 for size in args.mib):
        parser.error('sizes must be positive')
    args.output_dir.mkdir(parents=True, exist_ok=True)
    manifest = {'generator_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                'scope': __doc__, 'files': []}
    instructions = MIXED.splitlines()
    commented = ''.join(f'\t; v{i} = compiler-like operation; the comment payload is not machine code\n\t{op}\n'
                        for i, op in enumerate(instructions)) * 16
    for size in args.mib:
        for kind in ['instructions', 'functions']:
            path = args.output_dir / f'{kind}-{size}MiB.s'
            digest = hashlib.sha256()
            length = count = 0
            with path.open('wb') as output:
                def emit(text):
                    nonlocal length
                    data = text.encode()
                    output.write(data)
                    digest.update(data)
                    length += len(data)

                emit('.text\n' + ('.subsections_via_symbols\n' if kind == 'functions' else ''))
                while length < size * 1024 * 1024:
                    if kind == 'instructions':
                        emit(MIXED * 4096)
                    else:
                        emit(f'.globl _scale_{count}\n.p2align 2\n_scale_{count}:\n'
                             '.cfi_startproc\nstp x29, x30, [sp, #-16]!\nmov x29, sp\n'
                             '.cfi_def_cfa x29, 16\n.cfi_offset x29, -16\n.cfi_offset x30, -8\n'
                             f'Lscale_{count}_loop:\n' + commented +
                             f'cbnz x0, Lscale_{count}_loop\nbl _scale_external\n'
                             'ldp x29, x30, [sp], #16\nret\n.cfi_endproc\n')
                    count += 1
            row = {'path': str(path.resolve()), 'kind': kind, 'source_bytes': length,
                   'sha256': digest.hexdigest(), 'generation_units': count,
                   'instruction_count': count * (32768 if kind == 'instructions' else 134)}
            manifest['files'].append(row)
            print(row, flush=True)
    (args.output_dir / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')


if __name__ == '__main__':
    main()
