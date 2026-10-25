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
ARITHMETIC_NAMES += ["REV32_64_dp_1src", "SMULH_64_dp_3src", "UMULH_64_dp_3src", "SMADDL_64WA_dp_3src", "UMADDL_64WA_dp_3src",
                     "SMSUBL_64WA_dp_3src", "UMSUBL_64WA_dp_3src"]
CONTROL_NAMES = ["BR_64_branch_reg", "BLR_64_branch_reg", "BRK_EX_exception", "SVC_EX_exception", "B_only_condbranch", "TBZ_only_testbranch", "TBNZ_only_testbranch"]
CONTROL_NAMES += [f"{op}_{bits}_compbranch" for op in ("CBZ", "CBNZ") for bits in (32, 64)]
FP_NAMES = []
for tag in ("S", "D"):
    FP_NAMES += [f"{op}_{tag}_floatdp1" for op in ("FMOV", "FABS", "FNEG", "FSQRT", "FRINTN", "FRINTP", "FRINTM", "FRINTZ", "FRINTA", "FRINTX")]
    FP_NAMES += [f"{op}_{tag}_floatdp2" for op in ("FADD", "FSUB", "FMUL", "FDIV", "FMIN", "FMAX", "FMINNM", "FMAXNM")]
    FP_NAMES += [f"{op}_{tag}_floatdp3" for op in ("FMADD", "FMSUB", "FNMADD", "FNMSUB")]
    FP_NAMES += [f"{op}_{tag}{zero}_floatcmp" for op in ("FCMP", "FCMPE") for zero in ("", "Z")]
    FP_NAMES += [f"FCSEL_{tag}_floatsel", f"FMOV_{tag}_floatimm"]
    FP_NAMES += [f"{op}_{tag}{bits}_float2int" for op in ("SCVTF", "UCVTF") for bits in (32, 64)]
    FP_NAMES += [f"{op}_{bits}{tag}_float2int" for op in ("FCVTZS", "FCVTZU") for bits in (32, 64)]
FP_NAMES += ["FCVT_DS_floatdp1", "FCVT_SD_floatdp1", "FCVT_HS_floatdp1", "FCVT_HD_floatdp1", "FCVT_SH_floatdp1", "FCVT_DH_floatdp1", "FMOV_32S_float2int", "FMOV_S32_float2int", "FMOV_64D_float2int", "FMOV_D64_float2int"]
SIMD_NAMES = [f"{op}_asimdsame_only" for op in (
    "ADD", "SUB", "MUL", "MLA", "MLS", "SMAX", "SMIN", "UMAX", "UMIN",
    "CMEQ", "CMGT", "CMGE", "CMHI", "CMHS", "AND", "ORR", "ORN", "EOR",
    "BIC", "BSL", "BIT", "BIF", "FADD", "FSUB", "FMUL", "FDIV", "FMLA",
    "FMLS", "FMIN", "FMAX", "FMINNM", "FMAXNM")]
SIMD_NAMES += [f"{op}_asimdmisc_R" for op in ("ABS", "NEG", "NOT", "FABS", "FNEG", "FSQRT")]
SCALABLE_NAMES = ["SMSTART", "SMSTOP", "zero_za_i_", "ptrue_p_s_", "pfalse_p_", "rdvl_r_i_", "dup_z_i_", "lastb_r_p_z_"]
SCALABLE_NAMES += [f"{op}_z_zz_" for op in ("add", "sub", "and", "orr", "eor", "fadd", "fsub", "fmul")]
SCALABLE_NAMES += [f"{op}_r_s_" for op in ("cntb", "cnth", "cntw", "cntd")]
SCALABLE_NAMES += [f"{op}_za_pp_zz_32" for op in ("fmopa", "fmops")]
# Contiguous scalar-address Z memory (including extending loads/truncating
# stores), ordinary tail predicates, and vector-length/pointer helpers.
STREAMING_NAMES = [f"{op}_p_p_rr_" for op in ("whilelt", "whilelo", "whilele", "whilels")]
STREAMING_NAMES += ["addvl_r_ri_", "dup_z_r_"]
for op, sizes in (("ld1b", (8,16,32,64)), ("ld1h", (16,32,64)),
                  ("ld1w", (32,64)), ("ld1d", (64,)),
                  ("ld1sb", (16,32,64)), ("ld1sh", (32,64)), ("ld1sw", (64,))):
    STREAMING_NAMES += [f"{op}_z_p_{mode}_{'s' if op in ('ld1sb','ld1sh','ld1sw') else 'u'}{bits}"
                        for mode in ("bi", "br") for bits in sizes]
STREAMING_NAMES += [f"{op}_z_p_{mode}_" for op in ("st1b", "st1h", "st1w", "st1d") for mode in ("bi", "br")]
NEON_NAMES = [line for line in (ROOT / "isa/neon.txt").read_text().splitlines()
              if line and not line.startswith("#")]
NUMERIC_FEATURES = {"FEAT_FP16", "FEAT_FHM", "FEAT_BF16", "FEAT_DotProd", "FEAT_I8MM",
                    "FEAT_RDM", "FEAT_FCMA", "FEAT_JSCVT", "FEAT_FRINTTS"}
NUMERIC_NAMES = [line for line in (ROOT / "isa/m5-numeric.txt").read_text().splitlines()
                 if line and not line.startswith("#")]
SME_FEATURES = {"FEAT_SME2", "FEAT_SME_F64F64", "FEAT_SME_I16I64", "FEAT_SME_F16F16", "FEAT_SME_B16B16"}
SME_NAMES = [line for line in (ROOT / "isa/sme.txt").read_text().splitlines()
             if line and not line.startswith("#")]
COMPAT_FEATURES = {"FEAT_PAuth", "FEAT_BTI", "FEAT_AES", "FEAT_SHA1", "FEAT_SHA256", "FEAT_CRC32", "FEAT_PMULL", "FEAT_CSSC", "FEAT_LRCPC", "FEAT_LRCPC2"}
COMPAT_NAMES = [line for line in (ROOT / "isa/m5-compat.txt").read_text().splitlines()
                if line and not line.startswith("#")]
SYSTEM_NAMES = {"NZCV", "FPCR", "FPSR", "TPIDR_EL0", "TPIDRRO_EL0", "CNTVCT_EL0", "CNTPCT_EL0", "CNTFRQ_EL0", "CTR_EL0", "DCZID_EL0", "DC CVAU", "DC CVAC", "DC CIVAC", "DC ZVA", "IC IVAU", "IC IALLU", "IC IALLUIS"}
NAMES = sorted(set(
    [f"{op}_{bits}_movewide" for op in ("MOVZ", "MOVK", "MOVN") for bits in (32, 64)]
    + [f"{op}_{bits}{'S' if op.endswith('S') and form == 'imm' else ''}_addsub_{form}"
       for op in ("ADD", "SUB", "ADDS", "SUBS") for bits in (32, 64) for form in ("imm", "shift")]
    + [f"ORR_{bits}_log_shift" for bits in (32, 64)]
    + ["B_only_branch_imm", "BL_only_branch_imm", "RET_64R_branch_reg", "NOP_HI_hints"]
    + COMPAT_NAMES + STREAMING_NAMES + SME_NAMES + NUMERIC_NAMES + NEON_NAMES + SCALABLE_NAMES + EXCLUSIVE_NAMES + LITERAL_NAMES + SIMD_NAMES + ATOMIC_NAMES + BARRIER_NAMES + MEMORY_NAMES + ARITHMETIC_NAMES + CONTROL_NAMES + FP_NAMES + ["CNT_asimdmisc_R", "ADDV_asimdall_only", "MOVI_asimdimm_L_sl", "MOVI_asimdimm_L_hl", "MOVI_asimdimm_M_sm", "MOVI_asimdimm_N_b", "MOVI_asimdimm_D_ds", "MOVI_asimdimm_D2_d", "ADR_only_pcreladdr", "ADRP_only_pcreladdr"]
))


def walk(node, parent=None):
    if node.get("_type") == "Instruction.Instruction":
        yield node
        parent = node
    elif node.get("_type") == "Instruction.InstructionAlias" and node["name"] in ("SMSTART", "SMSTOP"):
        # Preserve the alias condition, including FEAT_SME, with the exact
        # parent encoding. Record where that encoding came from for auditing.
        yield dict(node, encoding=parent["encoding"], encoding_source=parent["name"])
    for child in node.get("children", []):
        yield from walk(child, parent)


def import_archive(path):
    assert hashlib.sha256(path.read_bytes()).hexdigest() == SHA256, "archive checksum mismatch"
    with tarfile.open(path) as archive:
        data = json.load(archive.extractfile("Instructions.json"))
        registers = json.load(archive.extractfile("Registers.json"))
    system = [{"name": r["name"], "condition": r["condition"],
               "accessors": [{k: a[k] for k in ("name", "condition", "encoding")} for a in r["accessors"]]}
              for r in registers if r["name"] in SYSTEM_NAMES and r["state"] == "AArch64"]
    assert {r["name"] for r in system} == SYSTEM_NAMES
    (ROOT / "isa/system.json").write_text(json.dumps(system, indent=2) + "\n")
    assert data["_meta"]["version"]["schema"] == "2.7.4"
    a64 = next(x for x in data["instructions"] if x["name"] == "A64")
    records = {x["name"]: x for x in walk(a64)}
    def feature_names(node):
        result = set()
        if isinstance(node, dict):
            if node.get("_type") == "AST.Identifier" and node["value"].startswith("FEAT_"):
                result.add(node["value"])
            for value in node.values():
                result |= feature_names(value)
        elif isinstance(node, list):
            for value in node:
                result |= feature_names(value)
        return result
    baseline_neon = sorted(name for name, record in records.items()
                           if "_" in name and name.split("_")[1].startswith(("asimd", "asisd"))
                           and feature_names(record["condition"]) == {"FEAT_AdvSIMD"})
    assert NEON_NAMES == baseline_neon, "baseline NEON inventory differs from pinned Arm archive"
    numeric = sorted(name for name, record in records.items()
                     if feature_names(record["condition"]) & NUMERIC_FEATURES
                     and feature_names(record["condition"]) <= NUMERIC_FEATURES | {"FEAT_FP", "FEAT_AdvSIMD"})
    assert NUMERIC_NAMES == numeric, "M5 numeric inventory differs from pinned Arm archive"


    subset = {
        "source_url": URL, "archive_sha256": SHA256, "_meta": data["_meta"],
        "instructions": [{k: records[name][k] for k in ("name", "condition", "encoding", "encoding_source") if k in records[name]} for name in NAMES],
    }
    (ROOT / "isa/a64.json").write_text(json.dumps(subset, indent=2) + "\n")


def requirements(node):
    """Translate understood predicates, retaining AND/OR/NOT and masked sets.

    Unknown AST forms, features, and predicate fields still fail closed.
    """
    if node == {"_type": "AST.Bool", "value": True}:
        return []
    if node.get("_type") == "AST.Function":
        assert node["name"] == "IsFeatureImplemented" and node["parameters"] == []
        assert len(node["arguments"]) == 1 and node["arguments"][0]["_type"] == "AST.Identifier"
        feature = node["arguments"][0]["value"]
        assert feature in {"FEAT_FP", "FEAT_AdvSIMD", "FEAT_LSE", "FEAT_SVE", "FEAT_SME"} | NUMERIC_FEATURES | SME_FEATURES | COMPAT_FEATURES
        return [f'Requirement::Feature("{feature}")']
    if node.get("_type") == "AST.UnaryOp" and node["op"] == "!":
        return [f'Requirement::Not(&Requirement::All(&[{", ".join(requirements(node["expr"]))}]))']
    if node.get("_type") == "AST.BinaryOp":
        if node["op"] == "&&":
            return requirements(node["left"]) + requirements(node["right"])
        if node["op"] == "||":
            sides = [f'Requirement::All(&[{", ".join(requirements(node[side]))}])' for side in ("left", "right")]
            return [f'Requirement::Any(&[{", ".join(sides)}])']
        if node["op"] in ("==", "!=", "IN"):
            assert node["left"]["_type"] == "AST.Identifier"
            field = node["left"]["value"]
            assert field in ("option", "size", "op1", "op2", "CRm", "immh", "Rm", "Rt")
            right = node["right"]
            if node["op"] == "IN":
                assert right["_type"] == "AST.Set" and right["values"]
                values = right["values"]
            else:
                values = [right]
            matches = []
            for value in values:
                assert value["_type"] == "Values.Value" and value["meaning"] is None
                bits = value["value"]
                assert bits.startswith("'") and bits.endswith("'")
                bits = bits[1:-1]
                assert bits and set(bits) <= set("01x")
                number = int(bits.replace("x", "0"), 2)
                mask = int("".join("0" if b == "x" else "1" for b in bits), 2)
                if node["op"] == "!=":
                    assert "x" not in bits
                    matches.append(f'Requirement::NotEqual(FieldId::{field}, {number})')
                else:
                    matches.append(f'Requirement::MaskedEqual(FieldId::{field}, {number}, {mask})')
            return [f'Requirement::Any(&[{", ".join(matches)}])'] if node["op"] == "IN" else matches

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
        "use keel_asm_ir::FieldId;",
    ]
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
    lines.append("pub(super) fn system_operand(op: &str, name: &str) -> Option<&'static [(FieldId, u32)]> { Some(match (op, name) {")
    for register in json.loads((ROOT / "isa/system.json").read_text()):
        assert register["condition"] == {"_type": "AST.Function", "arguments": [{"_type": "AST.Identifier", "value": "FEAT_AA64"}], "name": "IsFeatureImplemented", "parameters": []}
        for accessor in register["accessors"]:
            op = {"A64.MRS": "mrs", "A64.MSRregister": "msr", "A64.DC": "dc", "A64.IC": "ic"}[accessor["name"]]
            assert accessor["condition"] == {"_type": "AST.Bool", "value": True}
            assert len(accessor["encoding"]) == 1
            encoding = accessor["encoding"][0]
            fields = []
            for field, value in encoding["encodings"].items():
                bits = value["value"].strip("'")
                assert set(bits) <= set("01")
                number = int(bits, 2)
                if field == "op0":
                    if op in ("dc", "ic"):
                        assert number == 1
                        continue
                    assert number in (2, 3)
                    field, number = "o0", number - 2
                fields.append(f"(FieldId::{field}, {number})")
            lines.append(f'("{op}", "{encoding["asmvalue"].lower()}") => &[{", ".join(fields)}],')
    lines.append("_ => return None, }) }")
    # Keep dispatch and records in the same order, generated from one source.
    # Rust lowers this match into a decision tree rather than scanning names.
    lines.append("pub(super) fn encoding(name: &str) -> Option<&'static Encoding> {\nSome(match name {")
    for index, instruction in enumerate(data["instructions"]):
        lines.append(f'"{instruction["name"]}" => &ENCODINGS[{index}],')
    lines.append("_ => return None,\n})\n}\n")
    return subprocess.run(["rustfmt", "--edition", "2024"], input="\n".join(lines),
                          text=True, capture_output=True, check=True).stdout


def generate_ids(data):
    fields = sorted({f['name'] for i in data['instructions'] for f in i['encoding']['values']
                     if 'x' in f['value']['value']})
    lines = ['// Generated from Arm AARCHMRS 2025-12 by keel-asm/tools/generate.py.',
             '// Copyright (c) 2010-2025 Arm Limited or its affiliates. All rights reserved.',
             '// Arm-derived data: BSD-3-Clause; see ../LICENSE-ARM.']
    for kind, names in [('FieldId', fields), ('Opcode', [i['name'] for i in data['instructions']])]:
        lines += ['#[allow(non_camel_case_types)]', '#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]',
                  '#[repr(u16)]', f'pub enum {kind} {{' + ','.join(names) + '}', f'impl {kind} {{',
                  'pub fn name(self) -> &\'static str { match self {']
        lines += [f'Self::{name} => "{name}",' for name in names]
        lines += ['} }']
        if kind == 'Opcode':
            lines += ['pub fn index(self) -> usize { self as usize }',
                      'pub const ALL: &\'static [Self] = &[' + ','.join(f'Self::{n}' for n in names) + '];']
        lines += ['}', f'impl std::fmt::Display for {kind} {{',
                  'fn fmt(&self, f: &mut std::fmt::Formatter<\'_>) -> std::fmt::Result { f.write_str(self.name()) }', '}']
    return subprocess.run(['rustfmt', '--edition', '2024'], input='\n'.join(lines), text=True,
                          capture_output=True, check=True).stdout


def generate_decoder(data):
    """Share the inventory without linking the assembler into the decoder.

    Features are treated as present, not as a target CPU validity check.
    Field conditions retain their complete Boolean structure.
    """
    lines = ['// Generated by keel-asm/tools/generate.py; do not edit.',
             '// Copyright (c) 2010-2025 Arm Limited or its affiliates. All rights reserved.',
             '// Arm-derived data: BSD-3-Clause; see ../LICENSE-ARM.',
             'use super::{Encoding, Field};',
             'pub(super) static ENCODINGS: &[Encoding] = &[']
    for instruction in data['instructions']:
        requirements(instruction['condition'])  # Fail closed on new AST forms.
        fields = instruction['encoding']['values']
        by_name = {f['name']: f for f in fields if 'name' in f}
        predicate_mask = 0

        def condition(node):
            nonlocal predicate_mask
            kind = node['_type']
            if kind == 'AST.Bool':
                return str(node['value']).lower()
            if kind == 'AST.Function':
                return 'true'
            if kind == 'AST.UnaryOp':
                return f'!({condition(node["expr"])})'
            op = node['op']
            if op in ('&&', '||'):
                return f'({condition(node["left"])}) {op} ({condition(node["right"])})'
            field = by_name[node['left']['value']]['range']
            start, width = field['start'], field['width']
            predicate_mask |= ((1 << width) - 1) << start
            value = f'((word >> {start}) & {(1 << width) - 1})'
            values = node['right']['values'] if op == 'IN' else [node['right']]
            terms = []
            for item in values:
                bits = item['value'].strip("'")
                mask = int(''.join('0' if b == 'x' else '1' for b in bits), 2)
                number = int(bits.replace('x', '0'), 2)
                terms.append(f'({value} & {mask}) {"!=" if op == "!=" else "=="} {number}')
            return terms[0] if len(terms) == 1 else '(' + ') || ('.join(terms) + ')'

        predicate = condition(instruction['condition'])
        mask = base = 0
        for field in fields:
            for offset, bit in enumerate(reversed(field['value']['value'].strip("'"))):
                if bit != 'x':
                    mask |= 1 << (field['range']['start'] + offset)
                    base |= int(bit) << (field['range']['start'] + offset)
        lines.append(f'Encoding {{ name: "{instruction["name"]}", mask: {mask}, base: {base}, predicate_mask: {predicate_mask}, predicate: |word| {{ let _ = word; {predicate} }}, fields: &[')
        for field in fields:
            if 'name' in field:
                r = field['range']
                lines.append(f'Field {{ name: "{field["name"]}", start: {r["start"]}, width: {r["width"]} }},')
        lines.append('] },')
    lines.append('];')
    return subprocess.run(['rustfmt', '--edition', '2024'], input='\n'.join(lines), text=True,
                          capture_output=True, check=True).stdout


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, help="import the checksum-verified upstream tar.gz")
    parser.add_argument("--check", action="store_true", help="fail if checked-in Rust differs")
    args = parser.parse_args()
    if args.archive:
        import_archive(args.archive)
    data = json.loads((ROOT / "isa/a64.json").read_text())
    for target, output in [(ROOT / 'src/generated.rs', generate(data)),
                           (ROOT.parent / 'keel-asm-ir/src/generated.rs', generate_ids(data)),
                           (ROOT.parent / 'keel-disasm/src/generated.rs', generate_decoder(data))]:
        if args.check:
            if target.read_text() != output:
                raise SystemExit(f'{target} is stale; run tools/generate.py')
        else:
            target.write_text(output)
