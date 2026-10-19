#!/usr/bin/env python3
"""Validate bounded SFrame metadata with installed libsframe and GNU readelf.

Run on x86-64 Linux under the caller's workload lock. Uses installed tools and
libraries only, custom startup and concrete DSOs; this is not a CRT/unwind test.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import struct
import subprocess


def sections(data):
    shoff = struct.unpack_from("<Q", data, 40)[0]
    count, strings = struct.unpack_from("<HH", data, 60)
    headers = [struct.unpack_from("<IIQQQQIIQQ", data, shoff + i * 64) for i in range(count)]
    names = data[headers[strings][4]:headers[strings][4] + headers[strings][5]]
    return {names[h[0]:].split(b"\0", 1)[0].decode(): h for h in headers}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--keel", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--libc", type=Path, default=Path("/usr/lib/libc.so.6"))
    p.add_argument("--libsframe", type=Path, default=Path("/usr/lib/libsframe.so.3"))
    p.add_argument("--crt1", type=Path, default=Path("/usr/lib/crt1.o"))
    args = p.parse_args()
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        p.error("run on x86-64 Linux")
    keel, out = args.keel.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    commands = []

    def run(argv, native=False, rc=0):
        result = subprocess.run([str(a) for a in argv], capture_output=True, timeout=60,
                                env=dict(os.environ, PATH="/nonexistent") if native else None)
        commands.append(dict(argv=[str(a) for a in argv], empty_path=native, rc=result.returncode,
                             stdout=result.stdout.decode(errors="replace"), stderr=result.stderr.decode(errors="replace")))
        (out / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        if rc is not None:
            assert result.returncode == rc, commands[-1]
        return result

    for tool in ["clang", "as", "ld", "readelf"]:
        run([tool, "--version"])
    run(["pacman", "-Q", "binutils"])
    start = out / "start.s"
    start.write_text('''
.text
.globl _start
_start:
    xor %ebp,%ebp
    and $-16,%rsp
    call main
    mov %eax,%edi
    call *exit@GOTPCREL(%rip)
    ud2
.section .note.GNU-stack,"",@progbits
''')
    metadata = out / "metadata.s"
    metadata.write_text('''
.text
.p2align 4
.globl alpha
alpha: ret
.zero 37
.p2align 4
.globl beta
beta: ret
.zero 4
.p2align 4
.globl gamma
gamma: ret
.zero 4
.section .sframe,"a",@0x6ffffff4
.p2align 3
.byte 0xe2,0xde,3,4,3,0,0xf8,0
.long 3,4,26,0,48
.quad alpha-.
.long 38,0
.quad beta-.
.long 5,10
.quad gamma-.
.long 5,18
.byte 2,0,0,0,0,0,3,8,4,0
.byte 1,0,0,0,0,0,3,8
.byte 1,0,0,0,0,0,3,8
.section .note.GNU-stack,"",@progbits
''')
    consumer = out / "consumer.c"
    consumer.write_text(r'''
#define _GNU_SOURCE
#include <link.h>
#include <stdint.h>
#include <sframe-api.h>
extern char alpha[], beta[], gamma[];
static const char *base;
static size_t length;
static int discover(struct dl_phdr_info *obj, size_t size, void *arg) {
    (void)size; (void)arg;
    if (obj->dlpi_name && obj->dlpi_name[0]) return 0;
    for (unsigned i = 0; i < obj->dlpi_phnum; ++i) {
        const ElfW(Phdr) *p = obj->dlpi_phdr + i;
        if (p->p_type == 0x6474e554) {
            if (base || p->p_flags != PF_R || p->p_align != 8 ||
                p->p_memsz != p->p_filesz) return 99;
            int loaded = 0;
            for (unsigned j = 0; j < obj->dlpi_phnum; ++j) {
                const ElfW(Phdr) *l = obj->dlpi_phdr + j;
                if (l->p_type == PT_LOAD && l->p_flags == PF_R &&
                    p->p_vaddr >= l->p_vaddr && p->p_offset >= l->p_offset &&
                    p->p_vaddr + p->p_memsz <= l->p_vaddr + l->p_memsz &&
                    p->p_offset + p->p_filesz <= l->p_offset + l->p_filesz &&
                    p->p_vaddr - l->p_vaddr == p->p_offset - l->p_offset) loaded = 1;
            }
            if (!loaded) return 98;
            base = (const char *)(obj->dlpi_addr + p->p_vaddr);
            length = p->p_filesz;
        }
        if (p->p_type == PT_PHDR &&
            (p->p_filesz != obj->dlpi_phnum * sizeof(*p) || p->p_memsz != p->p_filesz)) return 97;
    }
    return 0;
}
static int lookup(sframe_decoder_ctx *ctx, const char *pc, int undefined) {
    sframe_frame_row_entry row;
    if (sframe_find_fre(ctx, (int64_t)((uintptr_t)pc - (uintptr_t)base), &row)) return 1;
    int err = 0;
    if (sframe_fre_get_ra_undefined_p(ctx, &row, &err) != undefined || err) return 2;
    if (!undefined) {
        if (sframe_fre_get_base_reg_id(&row, &err) != SFRAME_BASE_REG_SP || err) return 3;
        if (sframe_fre_get_cfa_offset(ctx, &row, SFRAME_FDE_TYPE_DEFAULT, &err) != 8 || err) return 4;
        if (sframe_fre_get_ra_offset(ctx, &row, SFRAME_FDE_TYPE_DEFAULT, &err) != -8 || err) return 5;
    }
    return 0;
}
int main(void) {
    if (dl_iterate_phdr(discover, 0) || !base || length < 102) return 1;
    int err = 0;
    sframe_decoder_ctx *ctx = sframe_decode(base, length, &err);
    if (!ctx || err || sframe_decoder_get_version(ctx) != 3 ||
        sframe_decoder_get_abi_arch(ctx) != 3 || !(sframe_decoder_get_flags(ctx) & 4) ||
        sframe_decoder_get_num_fidx(ctx) < 3) return 2;
    const char *functions[] = {alpha, beta, gamma};
    const uint32_t sizes[] = {38, 5, 5}, rows[] = {2, 1, 1};
    for (unsigned i = 0; i < 3; ++i) {
        int found = 0;
        for (unsigned j = 0; j < sframe_decoder_get_num_fidx(ctx); ++j) {
            uint32_t n, size;
            int64_t delta;
            unsigned char info, info2, repeat;
            if (sframe_decoder_get_funcdesc_v3(ctx, j, &n, &size, &delta, &info, &info2, &repeat)) return 3;
            uint32_t field = sframe_decoder_get_offsetof_fde_start_addr(ctx, j, &err);
            if (err || field != 28 + 16 * j) return 4;
            if ((uintptr_t)base + field + delta == (uintptr_t)functions[i]) {
                if (found || n != rows[i] || size != sizes[i] || info || info2 || repeat) return 4;
                found = 1;
            }
        }
        if (!found) return 4;
        if (lookup(ctx, functions[i], 0) || lookup(ctx, functions[i] + sizes[i] - 1, i == 0)) return 5;
        sframe_frame_row_entry row;
        // End is exclusive; the alignment gaps have no FDE coverage.
        if (!sframe_find_fre(ctx, (int64_t)((uintptr_t)(functions[i] + sizes[i]) - (uintptr_t)base), &row)) return 6;
    }
    if (lookup(ctx, alpha + 3, 0) || lookup(ctx, alpha + 4, 1)) return 7;
    sframe_decoder_free(&ctx);
    return 42;
}
''')
    run(["clang", "-c", start, "-o", out / "start.o"])
    run(["as", "-mx86-used-note=no", metadata, "-o", out / "metadata.o"])
    run(["clang", "-O2", "-fPIC", "-fno-plt", "-c", consumer, "-o", out / "consumer.o"])
    inputs = [out / "start.o", out / "metadata.o", out / "consumer.o", args.libsframe, args.libc]
    native, reference = out / "consumer.keel", out / "consumer.gnu"
    run([keel, "link", "--target", "x86_64-unknown-linux-gnu", *inputs, "-o", native], True)
    run(["ld", "--dynamic-linker", "/lib64/ld-linux-x86-64.so.2", "-o", reference, *inputs])
    for exe in [native, reference]:
        run([exe], rc=42)
        (out / (exe.name + ".headers.txt")).write_bytes(run(["readelf", "-lWS", exe]).stdout)
        (out / (exe.name + ".sframe.txt")).write_bytes(run(["readelf", "--sframe", exe]).stdout)
    # Preserve every row byte and geometry; only start fields and SORTED differ.
    original = (out / "metadata.o").read_bytes()
    source = sections(original)[".sframe"]
    source_data = original[source[4]:source[4] + source[5]]
    # Structural check against the immutable, real installed CRT. No CRT input
    # is stripped or substituted into a claimed full-CRT link in this stage.
    crt = args.crt1.read_bytes()
    crt_section = sections(crt)[".sframe"]
    assert crt[crt_section[4]:crt_section[4] + crt_section[5]] == source_data
    rel = sections(crt)[".rela.sframe"]
    crt_relocs = [struct.unpack_from("<QQq", crt, rel[4] + i * 24) for i in range(rel[5] // 24)]
    assert [(offset, info & 0xffffffff, addend) for offset, info, addend in crt_relocs] == [(28, 24, 0), (44, 24, 48), (60, 24, 64)]
    (out / "crt1.sframe.txt").write_bytes(run(["readelf", "--sframe", args.crt1]).stdout)
    spans = {}
    for exe in [native, reference]:
        data = exe.read_bytes()
        s = sections(data)[".sframe"]
        sf = data[s[4]:s[4] + s[5]]
        assert s[1:3] == (0x6ffffff4, 2)
        assert sf[3] == 5
        count = struct.unpack_from("<I", sf, 8)[0]
        fre_len, fdeoff, freoff = struct.unpack_from("<III", sf, 16)
        assert fdeoff == 0 and freoff == 16 * count and 28 + freoff + fre_len == len(sf)
        # GNU may add PLT records. Both outputs must preserve each admitted block.
        admitted = [i for i in range(count) if struct.unpack_from("<I", sf, 28 + 16 * i + 8)[0] in (38, 5)]
        assert len(admitted) == 3
        for i, original_offset, size in zip(admitted, (0, 10, 18), (10, 8, 8)):
            offset = struct.unpack_from("<I", sf, 28 + 16 * i + 12)[0]
            assert sf[28 + freoff + offset:28 + freoff + offset + size] == source_data[76 + original_offset:76 + original_offset + size]
        phoff = struct.unpack_from("<Q", data, 32)[0]
        phnum = struct.unpack_from("<H", data, 56)[0]
        phs = [struct.unpack_from("<IIQQQQQQ", data, phoff + i * 56) for i in range(phnum)]
        frame_headers = [h for h in phs if h[0] == 0x6474e554]
        assert len(frame_headers) == 1
        h = frame_headers[0]
        assert h[:5] == (0x6474e554, 4, s[4], s[3], s[3]) and h[5] == h[6] and h[7] == 8
        spans[exe.name] = dict(section_size=s[5], segment_size=h[5], fdes=count)
        if exe == native:
            assert s[5] == 102 and h[5] == 102 and count == 3
        else:
            # Installed GNU ld reserves extra segment bytes for generated PLT
            # records. Record that discrepancy; Keel must have exact sizes.
            assert h[5] >= s[5]
    # Separate controls prove discovery and sorted lookup matter to the consumer.
    data = bytearray(native.read_bytes())
    phoff = struct.unpack_from("<Q", data, 32)[0]
    phnum = struct.unpack_from("<H", data, 56)[0]
    for i in range(phnum):
        if struct.unpack_from("<I", data, phoff + i * 56)[0] == 0x6474e554:
            struct.pack_into("<I", data, phoff + i * 56, 0)
            break
    control = out / "no-advertisement"
    control.write_bytes(data); control.chmod(0o755)
    run([control], rc=1)
    data = bytearray(native.read_bytes()); s = sections(data)[".sframe"]
    data[s[4] + 3] = 4
    control = out / "no-sorted"
    control.write_bytes(data); control.chmod(0o755)
    run([control], rc=5)
    # A second real producer fails before replacing an existing output.
    second = out / "second.s"
    second.write_text(".text\n.globl extra\nextra:\n.cfi_startproc\nret\n.cfi_endproc\n.section .note.GNU-stack,\"\",@progbits\n")
    run(["as", "--gsframe", "-mx86-used-note=no", second, "-o", out / "second.o"])
    preserved = out / "preserved"
    preserved.write_bytes(b"existing output\n")
    result = run([keel, "link", "--target", "x86_64-unknown-linux-gnu", *inputs, out / "second.o", "-o", preserved], True, rc=None)
    assert result.returncode and b"second selected SFrame producer" in result.stderr
    assert preserved.read_bytes() == b"existing output\n"
    # Selection, rather than archive presence, determines producer count.
    run(["ar", "rcs", out / "unused.a", out / "second.o"])
    run([keel, "link", "--target", "x86_64-unknown-linux-gnu", *inputs, out / "unused.a", "-o", out / "unused.keel"], True)
    run([out / "unused.keel"], rc=42)
    (out / "report.json").write_text(json.dumps(dict(host=platform.node(),
        native_exit=42, gnu_exit=42, discovery_control=1, sorted_control=5,
        second_producer_rejected=True, unused_archive_ignored=True,
        installed_crt1_structure_matches=True, spans=spans,
        installed_header_sha256=hashlib.sha256(Path("/usr/include/sframe.h").read_bytes()).hexdigest(),
        installed_libsframe_sha256=hashlib.sha256(args.libsframe.read_bytes()).hexdigest(),
        scope="single AMD64 v3 producer, installed libsframe lookup; custom startup, no CRT or CFI unwinding claim"), indent=2) + "\n")
    print((out / "report.json").read_text())


if __name__ == "__main__":
    main()
