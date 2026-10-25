use crate::EncodingMatch;

fn reg(n: u32, bits: u32, sp: bool) -> String {
    match (n, bits, sp) {
        (31, 64, true) => "sp".into(),
        (31, _, true) => "wsp".into(),
        (31, 64, false) => "xzr".into(),
        (31, _, false) => "wzr".into(),
        (_, 64, _) => format!("x{n}"),
        _ => format!("w{n}"),
    }
}
fn signed(value: u32, width: u32) -> i64 {
    ((value as i64) << (64 - width)) >> (64 - width)
}
fn target(address: u64, delta: i64) -> String {
    format!("0x{:x}", address.wrapping_add_signed(delta))
}
const COND: [&str; 16] = [
    "eq", "ne", "hs", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "al", "nv",
];

pub(super) fn assembly(e: &EncodingMatch, address: u64) -> Option<String> {
    let f = |name| e.field(name).unwrap_or(0);
    let name = e.name;
    let op = name.split('_').next()?.to_ascii_lowercase();
    let bits = if name.split('_').nth(1)?.starts_with("32") {
        32
    } else {
        64
    };
    let rd = reg(f("Rd"), bits, false);
    let rn = reg(f("Rn"), bits, false);
    let rm = reg(f("Rm"), bits, false);
    if name == "NOP_HI_hints" {
        return Some("nop".into());
    }
    if name == "RET_64R_branch_reg" {
        return Some(if f("Rn") == 30 {
            "ret".into()
        } else {
            format!("ret {rn}")
        });
    }
    if name.ends_with("_branch_reg") && matches!(op.as_str(), "br" | "blr") {
        return Some(format!("{op} {rn}"));
    }
    if name.ends_with("_branch_imm") {
        return Some(format!(
            "{op} {}",
            target(address, signed(f("imm26"), 26) * 4)
        ));
    }
    if name == "B_only_condbranch" {
        return Some(format!(
            "b.{} {}",
            COND[f("cond") as usize],
            target(address, signed(f("imm19"), 19) * 4)
        ));
    }
    if name.ends_with("_compbranch") {
        return Some(format!(
            "{op} {}, {}",
            reg(f("Rt"), bits, false),
            target(address, signed(f("imm19"), 19) * 4)
        ));
    }
    if name.ends_with("_testbranch") {
        let bit = f("b5") * 32 + f("b40");
        return Some(format!(
            "{op} {}, #{bit}, {}",
            reg(f("Rt"), if bit < 32 { 32 } else { 64 }, false),
            target(address, signed(f("imm14"), 14) * 4)
        ));
    }
    if name.ends_with("_pcreladdr") {
        let offset = signed((f("immhi") << 2) | f("immlo"), 21);
        let dest = if op == "adrp" {
            target(address & !4095, offset * 4096)
        } else {
            target(address, offset)
        };
        return Some(format!("{op} {rd}, {dest}"));
    }
    if name.ends_with("_exception") && matches!(op.as_str(), "brk" | "svc") {
        return Some(format!("{op} #{}", f("imm16")));
    }
    if name.ends_with("_movewide") {
        let shift = f("hw") * 16;
        if shift >= bits {
            return None;
        }
        return Some(format!(
            "{op} {rd}, #{}{}",
            f("imm16"),
            if shift == 0 {
                String::new()
            } else {
                format!(", lsl #{shift}")
            }
        ));
    }
    if name.ends_with("_addsub_imm") {
        let set = op.ends_with('s');
        let dest = reg(f("Rd"), bits, !set);
        let src = reg(f("Rn"), bits, true);
        let imm = f("imm12");
        let shift = if f("sh") == 0 { "" } else { ", lsl #12" };
        if op == "add" && imm == 0 && f("sh") == 0 && (f("Rd") == 31 || f("Rn") == 31) {
            return Some(format!("mov {dest}, {src}"));
        }
        if set && f("Rd") == 31 {
            return Some(format!(
                "{} {src}, #{imm}{shift}",
                if op == "subs" { "cmp" } else { "cmn" }
            ));
        }
        return Some(format!("{op} {dest}, {src}, #{imm}{shift}"));
    }
    if name.ends_with("_addsub_shift") || name.ends_with("_log_shift") {
        let shift = f("shift");
        let amount = f("imm6");
        if amount >= bits || (shift == 3 && name.ends_with("_addsub_shift")) {
            return None;
        }
        let suffix = if amount == 0 && shift == 0 {
            String::new()
        } else {
            format!(
                ", {} #{amount}",
                ["lsl", "lsr", "asr", "ror"][shift as usize]
            )
        };
        if op == "orr" && f("Rn") == 31 && shift == 0 && amount == 0 {
            return Some(format!("mov {rd}, {rm}"));
        }
        if matches!(op.as_str(), "subs" | "adds" | "ands") && f("Rd") == 31 {
            let alias = match op.as_str() {
                "subs" => "cmp",
                "adds" => "cmn",
                _ => "tst",
            };
            return Some(format!("{alias} {rn}, {rm}{suffix}"));
        }
        return Some(format!("{op} {rd}, {rn}, {rm}{suffix}"));
    }
    if name.ends_with("_log_imm") {
        let n = f("N");
        if bits == 32 && n != 0 {
            return None;
        }
        let imms = f("imms");
        let value = (n << 6) | ((!imms) & 63);
        if value < 2 {
            return None;
        }
        let len = 31 - value.leading_zeros();
        let size = 1u32 << len;
        let levels = size - 1;
        let s = imms & levels;
        if s == levels {
            return None;
        }
        let r = f("immr") & levels;
        let ones = u64::MAX >> (63 - s);
        let mask = u64::MAX >> (64 - size);
        let rotated = if r == 0 {
            ones
        } else {
            ((ones >> r) | (ones << (size - r))) & mask
        };
        let mut imm = 0u64;
        for shift in (0..bits).step_by(size as usize) {
            imm |= rotated << shift;
        }
        if op == "ands" && f("Rd") == 31 {
            return Some(format!("tst {rn}, #0x{imm:x}"));
        }
        let dest = reg(f("Rd"), bits, op != "ands");
        return Some(format!("{op} {dest}, {rn}, #0x{imm:x}"));
    }
    if name.ends_with("_addsub_ext") {
        let option = f("option");
        let amount = f("imm3");
        if amount > 4 {
            return None;
        }
        let src = reg(f("Rn"), bits, true);
        let rhs = reg(
            f("Rm"),
            if option == 3 || option == 7 { 64 } else { 32 },
            false,
        );
        if bits == 32 && (option == 3 || option == 7) {
            return None;
        }
        let ext = [
            "uxtb", "uxth", "uxtw", "uxtx", "sxtb", "sxth", "sxtw", "sxtx",
        ][option as usize];
        let suffix = format!("{rhs}, {ext} #{amount}");
        if op.ends_with('s') && f("Rd") == 31 {
            return Some(format!(
                "{} {src}, {suffix}",
                if op == "subs" { "cmp" } else { "cmn" }
            ));
        }
        return Some(format!(
            "{op} {}, {src}, {suffix}",
            reg(f("Rd"), bits, !op.ends_with('s'))
        ));
    }
    if name.ends_with("_addsub_carry") || name.ends_with("_dp_2src") {
        if !matches!(
            op.as_str(),
            "adc" | "adcs" | "sbc" | "sbcs" | "sdiv" | "udiv" | "lslv" | "lsrv" | "asrv" | "rorv"
        ) {
            return None;
        }
        let mnemonic = match op.as_str() {
            "lslv" => "lsl",
            "lsrv" => "lsr",
            "asrv" => "asr",
            "rorv" => "ror",
            _ => &op,
        };
        return Some(format!("{mnemonic} {rd}, {rn}, {rm}"));
    }
    if name.ends_with("_dp_1src")
        && matches!(
            op.as_str(),
            "clz" | "cls" | "rev" | "rev16" | "rev32" | "rbit" | "abs"
        )
    {
        return Some(format!("{op} {rd}, {rn}"));
    }
    if name.ends_with("_dp_3src") {
        if matches!(op.as_str(), "smulh" | "umulh") {
            return Some(format!("{op} {rd}, {rn}, {rm}"));
        }
        if matches!(op.as_str(), "madd" | "msub") {
            if f("Ra") == 31 {
                return Some(format!(
                    "{} {rd}, {rn}, {rm}",
                    if op == "madd" { "mul" } else { "mneg" }
                ));
            }
            return Some(format!(
                "{op} {rd}, {rn}, {rm}, {}",
                reg(f("Ra"), bits, false)
            ));
        }
    }
    if name.ends_with("_condsel") {
        return Some(format!(
            "{op} {rd}, {rn}, {rm}, {}",
            COND[f("cond") as usize]
        ));
    }
    if name.ends_with("_bitfield") || name.ends_with("_extract") {
        if f("immr") >= bits || f("imms") >= bits || f("N") != u32::from(bits == 64) {
            return None;
        }
        return Some(if name.ends_with("_extract") {
            format!("extr {rd}, {rn}, {rm}, #{}", f("imms"))
        } else {
            format!("{op} {rd}, {rn}, #{}, #{}", f("immr"), f("imms"))
        });
    }
    if name.contains("_ldst_") || name.contains("_ldstpair_") || name.ends_with("_loadlit") {
        return memory(e, address);
    }
    // Scalar floating point only. Vector/SVE/SME formats remain explicit .inst.
    let tag = name.split('_').nth(1)?;
    if matches!(tag, "S" | "D") {
        let r = |field| format!("{}{n}", tag.to_ascii_lowercase(), n = f(field));
        if name.ends_with("_floatdp1")
            && matches!(
                op.as_str(),
                "fmov"
                    | "fabs"
                    | "fneg"
                    | "fsqrt"
                    | "frintn"
                    | "frintp"
                    | "frintm"
                    | "frintz"
                    | "frinta"
                    | "frintx"
            )
        {
            return Some(format!("{op} {}, {}", r("Rd"), r("Rn")));
        }
        if name.ends_with("_floatdp2") {
            return Some(format!("{op} {}, {}, {}", r("Rd"), r("Rn"), r("Rm")));
        }
        if name.ends_with("_floatdp3") {
            return Some(format!(
                "{op} {}, {}, {}, {}",
                r("Rd"),
                r("Rn"),
                r("Rm"),
                r("Ra")
            ));
        }
        if name.ends_with("_floatsel") {
            return Some(format!(
                "{op} {}, {}, {}, {}",
                r("Rd"),
                r("Rn"),
                r("Rm"),
                COND[f("cond") as usize]
            ));
        }
    }
    None
}

fn memory(e: &EncodingMatch, address: u64) -> Option<String> {
    let f = |name| e.field(name).unwrap_or(0);
    let mut parts = e.name.split('_');
    let op = parts.next()?.to_ascii_lowercase();
    let tag = parts.next()?;
    let (prefix, scale) = match tag {
        "32" => ("w", 2),
        "64" => ("x", 3),
        "B" => ("b", 0),
        "H" => ("h", 1),
        "S" => ("s", 2),
        "D" => ("d", 3),
        "Q" => ("q", 4),
        _ => return None,
    };
    if !matches!(
        op.as_str(),
        "ldr"
            | "str"
            | "ldur"
            | "stur"
            | "ldrb"
            | "strb"
            | "ldrh"
            | "strh"
            | "ldrsb"
            | "ldrsh"
            | "ldrsw"
            | "ldurb"
            | "sturb"
            | "ldurh"
            | "sturh"
            | "ldursb"
            | "ldursh"
            | "ldursw"
            | "ldp"
            | "stp"
    ) {
        return None;
    }
    let register = |n| {
        if prefix == "w" || prefix == "x" {
            reg(n, if prefix == "w" { 32 } else { 64 }, false)
        } else {
            format!("{prefix}{n}")
        }
    };
    let rt = register(f("Rt"));
    if e.name.ends_with("_loadlit") {
        return Some(format!(
            "{op} {rt}, {}",
            target(address, signed(f("imm19"), 19) * 4)
        ));
    }
    let base = reg(f("Rn"), 64, true);
    let pair = e.name.contains("_ldstpair_");
    let regs = if pair {
        format!("{rt}, {}", register(f("Rt2")))
    } else {
        rt
    };
    let form = e.name.rsplit('_').next()?;
    let access_scale = if op.ends_with('b') {
        0
    } else if op.ends_with('h') {
        1
    } else if op.ends_with("sw") {
        2
    } else {
        scale
    };
    let offset = match form {
        "pos" => i64::from(f("imm12")) << access_scale,
        "unscaled" | "immpre" | "immpost" => signed(f("imm9"), 9),
        "off" | "pre" | "post" if pair => signed(f("imm7"), 7) << scale,
        "regoff" => {
            let option = f("option");
            let ext = match option {
                2 => "uxtw",
                3 => "lsl",
                6 => "sxtw",
                7 => "sxtx",
                _ => return None,
            };
            let rm = reg(
                f("Rm"),
                if option == 2 || option == 6 { 32 } else { 64 },
                false,
            );
            let shift = if f("S") == 0 { 0 } else { access_scale };
            let suffix = if option == 3 && shift == 0 {
                String::new()
            } else {
                format!(", {ext} #{shift}")
            };
            return Some(format!("{op} {regs}, [{base}, {rm}{suffix}]"));
        }
        _ => return None,
    };
    Some(match form {
        "immpost" | "post" => format!("{op} {regs}, [{base}], #{offset}"),
        "immpre" | "pre" => format!("{op} {regs}, [{base}, #{offset}]!"),
        _ if offset == 0 => format!("{op} {regs}, [{base}]"),
        _ => format!("{op} {regs}, [{base}, #{offset}]"),
    })
}
