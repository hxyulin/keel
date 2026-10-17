"""Source-only fixture checks; no compiler, native execution, or timing.

The affine checks prove each actual straight-line add body has the documented
per-register increment. They do not certify assembler encodings or full ABI.
"""
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
U64 = (1 << 64) - 1
LEGACY_IDS = {
    "arm-address-load", "arm-dependent-add", "arm-independent-add", "arm-control",
    "x86-dependent-add", "x86-independent-add", "x86-control",
}


def manifests():
    # These unchanged source proofs describe exactly the original seven fixtures.
    # General parser/contract checks for every manifest live in test_more_patterns.
    return [json.loads((ROOT / "patterns" / f"{name}.json").read_text())
            for name in sorted(LEGACY_IDS)]


def body(source):
    return source.split("L_loop:\n", 1)[1].split(
        "    subs x0" if "subs x0" in source else "    subq $1, %rdi", 1)[0]


class PatternSources(unittest.TestCase):
    def test_manifest_paths_and_exact_uint64(self):
        cases = manifests()
        self.assertEqual(len(cases), 7)
        self.assertEqual(len({m["id"] for m in cases}), len(cases))
        for m in cases:
            self.assertEqual(m["schema"], 1)
            self.assertEqual(m["seeds"], [0, 1, U64])
            self.assertEqual(m["correctness_iterations"], [0, 1, 2, 17, 257])
            for n in m["seeds"] + m["correctness_iterations"] + [m["mask"]]:
                self.assertIs(type(n), int)
                self.assertTrue(0 <= n <= U64)
            records = m["variants"] + [m["oracle"]] + list(m["mca"].values())
            for entry in records:
                path = (ROOT / entry["source"]).resolve()
                self.assertTrue(path.is_relative_to(ROOT))
                self.assertTrue(path.is_file())
            self.assertEqual(set(m["mca"]), {v["id"] for v in m["variants"]})

    def test_mca_is_exact_body_without_loop_control(self):
        for m in manifests():
            for v in m["variants"]:
                asm = (ROOT / v["source"]).read_text()
                extract = (ROOT / m["mca"][v["id"]]["source"]).read_text()
                instructions = "\n".join(line for line in extract.splitlines()
                                         if not line.startswith("#")) + "\n"
                self.assertEqual(instructions, body(asm))
                self.assertNotRegex(instructions, r"\b(b\.ne|jnz|ret|subs|subq)\b")

    def test_actual_add_bodies_have_expected_independent_affine_steps(self):
        for m in manifests():
            if m["category"] not in {"latency", "throughput", "control"}:
                continue
            arm = m["arch"] == "aarch64"
            expected_regs = (["x4", "x5", "x6", "x7"] if arm else
                             ["%r8", "%r9", "%r10", "%r11"]) if m["category"] == "throughput" else (["x1"] if arm else ["%rsi"])
            increment = 1 if m["category"] == "control" else 2
            for v in m["variants"]:
                offsets = {reg: 0 for reg in expected_regs}
                for line in body((ROOT / v["source"]).read_text()).splitlines():
                    if arm:
                        match = re.fullmatch(r"    add (x\d+), (x\d+), #(\d+)", line)
                        self.assertIsNotNone(match)
                        dst, src, imm = match.groups()
                        self.assertEqual(dst, src)  # no cross-accumulator dependency
                    else:
                        match = re.fullmatch(r"    addq \$(\d+), (%r\w+)", line)
                        self.assertIsNotNone(match)
                        imm, dst = match.groups()
                    offsets[dst] += int(imm)
                self.assertEqual(offsets, dict.fromkeys(expected_regs, increment))
                self.assertEqual(sum(offsets.values()), m["work_per_iteration"])

    def test_four_accumulator_combine_modular_identity(self):
        # Algebra: sum(seed+j+2*n)-3*seed-6 = seed+8*n.
        # Coefficients prove the identity for all integers, hence mod 2^64.
        self.assertEqual((4 - 3, sum(range(4)) - 6, 4 * 2), (1, 0, 8))
        for seed in [0, 1, U64, U64 - 2, 1 << 63]:
            for n in [0, 1, 2, 257, U64]:
                combined = (sum((seed + j + 2*n) & U64 for j in range(4)) - 3*seed - 6) & U64
                self.assertEqual(combined, (seed + 8*n) & U64)
        for arch in ["arm", "x86"]:
            for variant in ["baseline", "candidate"]:
                source = (ROOT / f"asm/{arch}-independent-add-{variant}.S").read_text()
                if arch == "arm":
                    for line in ["mov x4, x1", "add x5, x1, #1", "add x6, x1, #2", "add x7, x1, #3", "add x8, x1, x1, lsl #1", "add x8, x8, #6", "add x1, x4, x5", "add x1, x1, x6", "add x1, x1, x7", "sub x1, x1, x8"]:
                        self.assertIn(line, source)
                else:
                    for line in ["movq %rsi, %r8", "leaq 1(%rsi), %r9", "leaq 2(%rsi), %r10", "leaq 3(%rsi), %r11", "leaq 6(%rsi,%rsi,2), %rax", "addq %r9, %r8", "addq %r10, %r8", "addq %r11, %r8", "subq %rax, %r8"]:
                        self.assertIn(line, source)

    def test_address_range_signed_unsigned_equivalence_and_storage(self):
        m = next(m for m in manifests() if m["id"] == "arm-address-load")
        self.assertEqual((m["data_words"], m["mask"]), (512, 255))
        for byte in range(256):
            signed = byte if byte < (1 << 31) else byte - (1 << 32)
            self.assertEqual(signed << 4, byte * 16)
            for mask in [0, 1, 127, 255, U64]:
                index = byte & mask
                self.assertLess(index * 16 + 7, m["data_words"] * 8)
                self.assertEqual(index * 16, (2 * index) * 8)
        # The byte range is essential: an arbitrary sign-bit-set W is unsafe.
        self.assertNotEqual((-1 << 4) & U64, (0xffffffff << 4) & U64)
        oracle = (ROOT / m["oracle"]["source"]).read_text()
        self.assertIn("((state >> 16) & UINT64_C(255)) & mask", oracle)
        self.assertIn("state = data[UINT64_C(2) * index]", oracle)
        for v in m["variants"]:
            src = (ROOT / v["source"]).read_text()
            self.assertIn("ubfx w4, w1, #16, #8", src)
            self.assertIn("and w4, w4, w3", src)
            self.assertIn("ldr x1, [x4]", src)

    def test_padding_changes_only_fold_and_preserves_instruction_slots(self):
        original = (ROOT / "asm/arm-address-load-original.S").read_text()
        padded = (ROOT / "asm/arm-address-load-paddedfold.S").read_text()
        compact = (ROOT / "asm/arm-address-load-compact.S").read_text()
        old = "    sxtw x4, w4\n    lsl x4, x4, #4\n    add x4, x2, x4\n"
        new = "    add x4, x2, w4, uxtw #4\n"
        self.assertEqual(original.replace(old, "    nop\n    nop\n" + new), padded)
        self.assertEqual(original.replace(old, new), compact)
        self.assertEqual(len(body(original).splitlines()), 6)
        self.assertEqual(len(body(compact).splitlines()), 4)

    def test_entry_guard_termination_and_leaf_abi_source_shape(self):
        for m in manifests():
            for v in m["variants"]:
                src = (ROOT / v["source"]).read_text()
                self.assertIn("#ifdef __APPLE__", src)
                self.assertIn("#define KERNEL _pattern_kernel", src)
                self.assertIn('.section .note.GNU-stack,"",', src)
                self.assertIn(".cfi_startproc", src)
                if m["arch"] == "aarch64":
                    self.assertIn("KERNEL:\n.cfi_startproc\n    cbz x0, L_done", src)
                    self.assertIn("subs x0, x0, #1\n    b.ne L_loop", src)
                    self.assertIn("L_done:\n    mov x0, x1\n    ret", src)
                    # All explicit register operands are caller-saved, no SP/frame.
                    regs = set(map(int, re.findall(r"\b[wx](\d+)\b", src)))
                    self.assertTrue(regs <= set(range(9)))
                else:
                    self.assertIn("testq %rdi, %rdi\n    jz L_done", src)
                    self.assertIn("subq $1, %rdi\n    jnz L_loop", src)
                    self.assertIn("L_done:\n    movq %rsi, %rax\n    ret", src)
                    regs = set(re.findall(r"%r\w+", src)) - {"%rprogbits"}
                    self.assertTrue(regs <= {"%rdi", "%rsi", "%rax", "%r8", "%r9", "%r10", "%r11"})
                self.assertNotRegex(src, r"\b(call|bl|push|pop|avx)\b")

    def test_mathematical_oracle_sources_and_control_identity(self):
        for name, factor in [("dependent-add", 2), ("independent-add", 8), ("control", 1)]:
            src = (ROOT / f"oracles/{name}.c").read_text()
            self.assertIn(f"return seed + UINT64_C({factor}) * iterations;", src)
            self.assertNotIn("while", src)
        for arch in ["arm", "x86"]:
            self.assertEqual((ROOT / f"asm/{arch}-control-baseline.S").read_bytes(),
                             (ROOT / f"asm/{arch}-control-candidate.S").read_bytes())


if __name__ == "__main__":
    unittest.main()
