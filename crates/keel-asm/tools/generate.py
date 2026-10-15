#!/usr/bin/env python3
"""Import a pinned Arm archive, or regenerate Rust from the checked-in subset."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]
URL = "https://developer.arm.com/-/cdn-downloads/permalink/Exploration-Tools-OS-Machine-Readable-Data/AARCHMRS_BSD/AARCHMRS_OPENSOURCE_A_profile_FAT-2025-12.tar.gz"
SHA256 = "4dc5da62a5c856d7b1086b895075f54807f821ea21a333049cb0f40f9479cecc"
# Scalar memory transfers, including separate byte register-offset forms.
SINGLE = [(op, tag) for op in ("LDR", "STR") for tag in ("32", "64", "B", "H", "S", "D", "Q")]
SINGLE += [(op, "32") for op in ("LDRB", "STRB", "LDRH", "STRH")]
SINGLE += [(op, tag) for op in ("LDRSB", "LDRSH") for tag in ("32", "64")]
SINGLE += [("LDRSW", "64")]
MEMORY_NAMES = []
for op, tag in SINGLE:
    MEMORY_NAMES += [f"{op}_{tag}_ldst_{form}" for form in ("pos", "immpre", "immpost")]
    unscaled = op.replace("LDR", "LDUR").replace("STR", "STUR")
    MEMORY_NAMES.append(f"{unscaled}_{tag}_ldst_unscaled")
    if tag == "B" or op in ("LDRB", "STRB", "LDRSB"):
        prefix = tag if tag == "B" else tag + "B"
        MEMORY_NAMES += [f"{op}_{prefix}_ldst_regoff", f"{op}_{prefix}L_ldst_regoff"]
    else:
        MEMORY_NAMES.append(f"{op}_{tag}_ldst_regoff")
MEMORY_NAMES += [f"{op}_{tag}_ldstpair_{form}" for op in ("LDP", "STP")
                 for tag in ("32", "64", "S", "D", "Q") for form in ("off", "pre", "post")]
MEMORY_NAMES += [f"{op}_{order}{bits}_ldstord" for op, order in (("LDAR", "LR"), ("STLR", "SL")) for bits in (32, 64)]
MEMORY_NAMES += [f"{op}{suffix}_{order}32_ldstord" for op, order in (("LDAR", "LR"), ("STLR", "SL")) for suffix in ("B", "H")]
ATOMIC_NAMES = [f"{op}{order}{suffix}_{'C' if op == 'CAS' else ''}{bits}_{'comswap' if op == 'CAS' else 'memop'}"
                for op in ("LDADD", "LDCLR", "LDSET", "LDEOR", "LDSMIN", "LDSMAX", "LDUMIN", "LDUMAX", "SWP", "CAS")
                for order in ("", "A", "L", "AL") for suffix, bits in (("B",32),("H",32),("",32),("",64))]
EXCLUSIVE_NAMES = [f"{op}{suffix}_{role}{bits}_ldstexclr"
                   for op, role in (("LDXR", "LR"), ("LDAXR", "LR"), ("STXR", "SR"), ("STLXR", "SR"))
                   for suffix, bits in (("B", 32), ("H", 32), ("", 32), ("", 64))]
EXCLUSIVE_NAMES += [f"{op}_{role}{bits}_ldstexclp"
                    for op, role in (("LDXP", "LP"), ("LDAXP", "LP"), ("STXP", "SP"), ("STLXP", "SP"))
                    for bits in (32, 64)]
LITERAL_NAMES = [f"LDR_{tag}_loadlit" for tag in ("32", "64", "S", "D", "Q")] + ["LDRSW_64_loadlit"]
BARRIER_NAMES = ["CLREX_BN_barriers", "DMB_BO_barriers", "DSB_BO_barriers", "ISB_BI_barriers"]
ARITHMETIC_NAMES = []
for bits in (32, 64):
    ARITHMETIC_NAMES += [f"{op}_{bits}_log_shift" for op in ("AND", "ANDS", "ORR", "EOR", "BIC", "BICS", "ORN", "EON")]
    ARITHMETIC_NAMES += [f"{op}_{bits}{'S' if op == 'ANDS' else ''}_log_imm" for op in ("AND", "ANDS", "ORR", "EOR")]
    ARITHMETIC_NAMES += [f"{op}_{bits}_addsub_carry" for op in ("ADC", "ADCS", "SBC", "SBCS")]
    ARITHMETIC_NAMES += [f"{op}_{bits}{'S' if op.endswith('S') else ''}_addsub_ext" for op in ("ADD", "ADDS", "SUB", "SUBS")]
    ARITHMETIC_NAMES += [f"{op}_{bits}_dp_2src" for op in ("SDIV", "UDIV", "LSLV", "LSRV", "ASRV", "RORV")]
    ARITHMETIC_NAMES += [f"{op}_{bits}_dp_1src" for op in ("CLZ", "CLS", "RBIT", "REV", "REV16")]
    ARITHMETIC_NAMES += [f"{op}_{bits}A_dp_3src" for op in ("MADD", "MSUB")]
    ARITHMETIC_NAMES += [f"{op}_{bits}_condsel" for op in ("CSEL", "CSINC", "CSINV", "CSNEG")]
    ARITHMETIC_NAMES += [f"{op}_{bits}_condcmp_{form}" for op in ("CCMP", "CCMN") for form in ("reg", "imm")]
    ARITHMETIC_NAMES += [f"{op}_{bits}M_bitfield" for op in ("SBFM", "UBFM", "BFM")]
    ARITHMETIC_NAMES += [f"EXTR_{bits}_extract"]
ARITHMETIC_NAMES += ["REV32_64_dp_1src", "SMULH_64_dp_3src", "UMULH_64_dp_3src", "SMADDL_64WA_dp_3src", "UMADDL_64WA_dp_3src"]
CONTROL_NAMES = ["BR_64_branch_reg", "BLR_64_branch_reg", "BRK_EX_exception", "SVC_EX_exception", "B_only_condbranch", "TBZ_only_testbranch", "TBNZ_only_testbranch"]
CONTROL_NAMES += [f"{op}_{bits}_compbranch" for op in ("CBZ", "CBNZ") for bits in (32, 64)]
FP_NAMES = []
for tag in ("S", "D"):
    FP_NAMES += [f"{op}_{tag}_floatdp1" for op in ("FMOV", "FABS", "FNEG", "FSQRT", "FRINTN", "FRINTP", "FRINTM", "FRINTZ")]
    FP_NAMES += [f"{op}_{tag}_floatdp2" for op in ("FADD", "FSUB", "FMUL", "FDIV", "FMIN", "FMAX")]
    FP_NAMES += [f"{op}_{tag}_floatdp3" for op in ("FMADD", "FMSUB", "FNMADD", "FNMSUB")]
    FP_NAMES += [f"{op}_{tag}{zero}_floatcmp" for op in ("FCMP", "FCMPE") for zero in ("", "Z")]
    FP_NAMES += [f"FCSEL_{tag}_floatsel", f"FMOV_{tag}_floatimm"]
    FP_NAMES += [f"{op}_{tag}{bits}_float2int" for op in ("SCVTF", "UCVTF") for bits in (32, 64)]
    FP_NAMES += [f"{op}_{bits}{tag}_float2int" for op in ("FCVTZS", "FCVTZU") for bits in (32, 64)]
FP_NAMES += ["FCVT_DS_floatdp1", "FCVT_SD_floatdp1", "FMOV_32S_float2int", "FMOV_S32_float2int", "FMOV_64D_float2int", "FMOV_D64_float2int"]
SIMD_NAMES = [f"{op}_asimdsame_only" for op in (
    "ADD", "SUB", "MUL", "MLA", "MLS", "SMAX", "SMIN", "UMAX", "UMIN",
    "CMEQ", "CMGT", "CMGE", "CMHI", "CMHS", "AND", "ORR", "ORN", "EOR",
    "BIC", "BSL", "BIT", "BIF", "FADD", "FSUB", "FMUL", "FDIV", "FMLA",
    "FMLS", "FMIN", "FMAX", "FMINNM", "FMAXNM")]
SIMD_NAMES += [f"{op}_asimdmisc_R" for op in ("ABS", "NEG", "NOT", "FABS", "FNEG", "FSQRT")]
NAMES = sorted(set(
    [f"{op}_{bits}_movewide" for op in ("MOVZ", "MOVK", "MOVN") for bits in (32, 64)]
    + [f"{op}_{bits}{'S' if op.endswith('S') and form == 'imm' else ''}_addsub_{form}"
       for op in ("ADD", "SUB", "ADDS", "SUBS") for bits in (32, 64) for form in ("imm", "shift")]
    + [f"ORR_{bits}_log_shift" for bits in (32, 64)]
    + ["B_only_branch_imm", "BL_only_branch_imm", "RET_64R_branch_reg", "NOP_HI_hints"]
    + EXCLUSIVE_NAMES + LITERAL_NAMES + SIMD_NAMES + ATOMIC_NAMES + BARRIER_NAMES + MEMORY_NAMES + ARITHMETIC_NAMES + CONTROL_NAMES + FP_NAMES + ["CNT_asimdmisc_R", "ADDV_asimdall_only", "MOVI_asimdimm_L_sl", "MOVI_asimdimm_L_hl", "MOVI_asimdimm_M_sm", "MOVI_asimdimm_N_b", "MOVI_asimdimm_D_ds", "MOVI_asimdimm_D2_d", "ADR_only_pcreladdr", "ADRP_only_pcreladdr"]
))


def walk(node):
    if node.get("_type") == "Instruction.Instruction":
        yield node
    for child in node.get("children", []):
        yield from walk(child)


def import_archive(path):
    assert hashlib.sha256(path.read_bytes()).hexdigest() == SHA256, "archive checksum mismatch"
    with tarfile.open(path) as archive:
        data = json.load(archive.extractfile("Instructions.json"))
    assert data["_meta"]["version"]["schema"] == "2.7.4"
    a64 = next(x for x in data["instructions"] if x["name"] == "A64")
    records = {x["name"]: x for x in walk(a64)}
    subset = {
        "source_url": URL, "archive_sha256": SHA256, "_meta": data["_meta"],
        "instructions": [{k: records[name][k] for k in ("name", "condition", "encoding")} for name in NAMES],
    }
    (ROOT / "isa/a64.json").write_text(json.dumps(subset, indent=2) + "\n")


def requirements(node):
    """Translate only understood conditions; unknown features/expressions fail."""
    if node == {"_type": "AST.Bool", "value": True}:
        return []
    if node.get("_type") == "AST.Function":
        assert node["name"] == "IsFeatureImplemented" and node["parameters"] == []
        assert len(node["arguments"]) == 1 and node["arguments"][0]["_type"] == "AST.Identifier"
        feature = node["arguments"][0]["value"]
        assert feature in ("FEAT_FP", "FEAT_AdvSIMD", "FEAT_LSE")
        return [f'Requirement::Feature("{feature}")']
    if node.get("_type") == "AST.BinaryOp":
        if node["op"] == "&&":
            return requirements(node["left"]) + requirements(node["right"])
        if node["op"] == "!=":
            assert node["left"] == {"_type": "AST.Identifier", "value": "option"}
            assert node["right"] == {"_type": "Values.Value", "meaning": None, "value": "'011'"}
            return ['Requirement::NotEqual(FieldId::option, 3)']
    raise AssertionError(f"unsupported ISA condition: {node}")


def generate(data):
    assert data["archive_sha256"] == SHA256
    assert [x["name"] for x in data["instructions"]] == NAMES
    lines = [
        "// Generated by tools/generate.py from Arm AARCHMRS 2025-12, schema 2.7.4.",
        "// Copyright (c) 2010-2025 Arm Limited or its affiliates. All rights reserved.",
        "// Arm-derived data: BSD-3-Clause; see ../isa/LICENSE-ARM.",
        "// Do not edit; regenerate from isa/a64.json.",
        "use super::{Encoding, Field, Requirement};",
    ]
    # Small numeric field identities avoid repeated string comparisons during
    # encoding. Keep names for diagnostics, generated from the same ISA source.
    field_names = sorted({field["name"] for instruction in data["instructions"]
                          for field in instruction["encoding"]["values"]
                          if "x" in field["value"]["value"]})
    declarations = ["#[allow(non_camel_case_types)]", "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
                    "pub(super) enum FieldId {" + ",".join(field_names) + "}",
                    "impl std::fmt::Display for FieldId {",
                    "fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {",
                    "f.write_str(match self {"]
    declarations += [f'Self::{name} => "{name}",' for name in field_names]
    declarations += ["})", "}", "}"]
    lines.extend(declarations)
    lines.append("pub(super) static ENCODINGS: &[Encoding] = &[")
    for instruction in data["instructions"]:
        conditions = requirements(instruction["condition"])
        encoding = instruction["encoding"]
        assert encoding["width"] == 32
        base = mask = covered = 0
        fields = []
        for field in encoding["values"]:
            start, width = field["range"]["start"], field["range"]["width"]
            assert 0 <= start < 32 and 0 < width <= 32 - start
            bits = field["value"]["value"]
            assert bits.startswith("'") and bits.endswith("'")
            bits = bits[1:-1]
            assert len(bits) == width and set(bits) <= set("01x")
            span = ((1 << width) - 1) << start
            assert not covered & span, "overlapping encoding fields"
            covered |= span
            for index, bit in enumerate(reversed(bits)):
                if bit != "x":
                    mask |= 1 << (start + index)
                    base |= int(bit) << (start + index)
            if "x" in bits:
                fields.append((field["name"], start, width))
        assert covered == 0xffffffff
        assert len({f[0] for f in fields}) == len(fields)
        lines.append(f'Encoding {{ name: "{instruction["name"]}", base: 0x{base:08x}, mask: 0x{mask:08x}, fields: &[')
        for name, start, width in fields:
            lines.append(f'Field {{ name: "{name}", id: FieldId::{name}, start: {start}, width: {width} }},')
        lines.append("], requirements: &[" + ", ".join(conditions) + "] },")
    lines.append("];\n")
    # Keep dispatch and records in the same order, generated from one source.
    # Rust lowers this match into a decision tree rather than scanning names.
    lines.append("pub(super) fn encoding(name: &str) -> Option<&'static Encoding> {\nSome(match name {")
    for index, instruction in enumerate(data["instructions"]):
        lines.append(f'"{instruction["name"]}" => &ENCODINGS[{index}],')
    lines.append("_ => return None,\n})\n}\n")
    return subprocess.run(["rustfmt", "--edition", "2024"], input="\n".join(lines),
                          text=True, capture_output=True, check=True).stdout


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, help="import the checksum-verified upstream tar.gz")
    parser.add_argument("--check", action="store_true", help="fail if checked-in Rust differs")
    args = parser.parse_args()
    if args.archive:
        import_archive(args.archive)
    output = generate(json.loads((ROOT / "isa/a64.json").read_text()))
    target = ROOT / "src/generated.rs"
    if args.check:
        if target.read_text() != output:
            raise SystemExit("generated.rs is stale; run tools/generate.py")
    else:
        target.write_text(output)
