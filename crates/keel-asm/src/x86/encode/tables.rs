//! Operand matching and emission driven by the shared generated ISA forms.
use super::*;
use keel_asm_ir::x86::{
    CoreOpcode, Repeat,
    isa::{self, IsaForm, IsaOperand},
};
use smallvec::SmallVec;

struct Dialect {
    name: String,
    original: String,
    byte: Option<u8>,
    direction: Option<usize>,
    size: Option<u16>,
    control: Option<u8>,
    wait: bool,
    address32: bool,
    repeat: u8,
    segment: Option<u8>,
    group: Option<u8>,
}

fn dialect(i: &Instruction) -> Dialect {
    let raw = if let Core(op) = i.opcode {
        op.mnemonic().to_owned()
    } else {
        i.opcode.to_string()
    };
    let mut d = Dialect {
        name: raw.clone(),
        original: String::new(),
        byte: None,
        direction: None,
        size: None,
        control: None,
        wait: false,
        address32: false,
        repeat: 0,
        segment: None,
        group: None,
    };
    if let Core(op) = i.opcode {
        match op {
            CoreOpcode::String {
                repeat,
                address32,
                segment,
                ..
            } => {
                d.name = op.mnemonic().into();
                d.address32 = address32;
                d.segment = segment;
                d.repeat = match repeat {
                    Repeat::None => 0,
                    Repeat::Rep => 0xf3,
                    Repeat::Repne => 0xf2,
                };
            }
            CoreOpcode::Loop { address32, .. } => d.address32 = address32,
            CoreOpcode::Xlat { address32, segment } => {
                d.address32 = address32;
                d.segment = segment;
            }
            CoreOpcode::Enter { operand16 } => d.size = Some(if operand16 { 16 } else { 64 }),
            CoreOpcode::RetFar { width } => {
                d.name = "ret".into();
                d.size = Some(width);
            }
            CoreOpcode::CallFar | CoreOpcode::JmpFar => {
                if let Some(M(m)) = i.operands.first() {
                    d.size = Some(match m.width {
                        32 => 16,
                        48 => 32,
                        80 => 64,
                        _ => 0,
                    });
                }
            }
            CoreOpcode::Pushw | CoreOpcode::Popw => {
                d.name.truncate(d.name.len() - 1);
                d.size = Some(16);
            }
            CoreOpcode::Int => {
                if matches!(i.operands.as_slice(),[I(a)] if constant(a)==Some(3)) {
                    d.name = "int3".into()
                }
            }
            _ => {}
        }
    }
    if let Fixed(op) = i.opcode {
        let (name, size, wait) = op.source();
        d.name = name.into();
        d.size = size;
        d.wait = wait;
    }
    if let Some((base, suffix)) = raw.split_once('.') {
        d.name = base.into();
        match suffix {
            "load" => d.direction = Some(0),
            "store" => d.direction = Some(1),
            "imm" => {
                // ModRM immediate selection below preserves both byte and
                // wider exact forms without duplicating their opcode bytes.
            }
            "6e" => d.byte = Some(0x6e),
            "7e" => d.byte = Some(0x7e),
            "d6" => d.byte = Some(0xd6),
            "7f" => d.byte = Some(0x7f),
            "s" => {
                if let V2(op) = i.opcode {
                    d.byte = Some(op.spec().byte)
                }
            }
            "short" => {}
            _ => {
                if base == "nop" {
                    let (byte, group) = suffix.split_once('.').unwrap_or((suffix, "0"));
                    d.byte = u8::from_str_radix(byte, 16).ok();
                    d.group = group.parse().ok();
                }
            }
        }
    }
    if raw == "movabs" || raw == "movabs32" {
        d.name = "mov".into();
        d.address32 = raw == "movabs32";
    }
    if matches!(i.opcode, Movq6e | Movq7e) {
        d.direction = Some(if i.opcode == Movq6e { 0 } else { 1 });
    }
    if matches!(
        i.opcode,
        Push | Pop | Core(CoreOpcode::PushRm | CoreOpcode::PopRm)
    ) {
        d.size = Some(
            if i.operands
                .iter()
                .any(|o| width(o) == 16 && !matches!(o,R(r) if r.class==RegisterClass::Segment))
            {
                16
            } else {
                64
            },
        );
    }
    if raw.ends_with("64") && (raw.starts_with("pcmp") || raw.starts_with("vpcmp")) {
        d.name.truncate(d.name.len() - 2);
        d.size = Some(64);
    }
    if raw.starts_with('f')
        && !raw.starts_with("fn")
        && matches!(raw.as_str(), "fstcw" | "fstsw" | "fstenv" | "fsave")
    {
        d.name = format!("fn{}", &raw[1..]);
        d.wait = true;
    }
    if matches!(
        raw.as_str(),
        "cmpeqps"
            | "cmpltps"
            | "cmpleps"
            | "cmpunordps"
            | "cmpneqps"
            | "cmpnltps"
            | "cmpnleps"
            | "cmpordps"
            | "cmpeqpd"
            | "cmpltpd"
            | "cmplepd"
            | "cmpunordpd"
            | "cmpneqpd"
            | "cmpnltpd"
            | "cmpnlepd"
            | "cmpordpd"
            | "cmpeqss"
            | "cmpltss"
            | "cmpless"
            | "cmpunordss"
            | "cmpneqss"
            | "cmpnltss"
            | "cmpnless"
            | "cmpordss"
            | "cmpeqsd"
            | "cmpltsd"
            | "cmplesd"
            | "cmpunordsd"
            | "cmpneqsd"
            | "cmpnltsd"
            | "cmpnlesd"
            | "cmpordsd"
    ) {
        let tag = &raw[raw.len() - 2..];
        d.name = format!("cmp{tag}");
        d.control = Some(match &raw[3..raw.len() - 2] {
            "eq" => 0,
            "lt" => 1,
            "le" => 2,
            "unord" => 3,
            "neq" => 4,
            "nlt" => 5,
            "nle" => 6,
            _ => 7,
        });
    }
    d.name = match d.name.as_str() {
        "callf" => "call",
        "jmpf" => "jmp",
        "fwait" => "fwait",
        "xlatb" => "xlat",
        "sal" => "shl",
        other => other,
    }
    .into();
    for stem in ["cmov", "set", "j"] {
        if let Some(cc) = d.name.strip_prefix(stem) {
            let canonical = keel_asm_ir::x86::isa::parse_condition_code(cc)
                .and_then(keel_asm_ir::x86::isa::isa_condition_code_name)
                .unwrap_or(cc);
            d.name = format!("{stem}{canonical}");
            break;
        }
    }
    d.original = raw;
    d
}

fn register_matches(spec: IsaOperand, r: Register, size: u16, address: u16) -> bool {
    let gpr = |bits| r.class == RegisterClass::Gpr && r.width == bits;
    match spec.kind {
        "gpr8" => gpr(8),
        "gpr16" => gpr(16),
        "gpr32" => gpr(32),
        "gpr64" => gpr(64),
        "gpr16_32_64" => gpr(size),
        "gpr32_32_64" => gpr(if size == 64 { 64 } else { 32 }),
        "gpr16_32_32" => gpr(if size == 16 { 16 } else { 32 }),
        "gpr_asz" => gpr(address),
        "implicit_reg" => {
            if spec.register == "st0" {
                return r.class == RegisterClass::X87 && r.number == 0;
            }
            let expected = match spec.register {
                "osz_ax" => reg_name(0, size),
                "asz_ax" => reg_name(0, address),
                other => other.into(),
            };
            r.to_string() == expected
        }
        kind => RegisterClass::from_isa_kind(kind).is_some_and(|class| r.class == class),
    }
}
fn reg_name(n: u8, bits: u16) -> String {
    Register {
        class: RegisterClass::Gpr,
        number: n,
        width: bits,
        high_byte: false,
    }
    .to_string()
}
fn matches_operand(spec: IsaOperand, o: &Operand, size: u16, address: u16) -> bool {
    match o {
        R(r) => register_matches(spec, *r, size, address),
        M(m) => {
            if !matches!(
                spec.kind,
                "mem" | "mem_vsibx" | "mem_vsiby" | "agen" | "agen_norel" | "moffs"
            ) {
                return false;
            }
            if spec.kind == "moffs" && (m.base.is_some() || m.index.is_some()) {
                return false;
            }
            if spec.kind == "agen_norel" && m.base.is_some_and(|r| r.class == RegisterClass::Rip) {
                return false;
            }
            if spec.kind == "mem_vsibx" && !m.index.is_some_and(|r| r.class == RegisterClass::Xmm) {
                return false;
            }
            if spec.kind == "mem_vsiby" && !m.index.is_some_and(|r| r.class == RegisterClass::Ymm) {
                return false;
            }
            m.width == 0
                || spec.bytes(size) == 0
                || m.width == spec.bytes(size) * 8
                || matches!(spec.kind, "agen" | "agen_norel")
        }
        I(a) => {
            if spec.kind == "implicit_imm1" {
                return constant(a) == Some(1);
            }
            if spec.kind == "rel" {
                return true;
            }
            if spec.kind != "imm" {
                return false;
            }
            let bytes = spec.bytes(size);
            if bytes == 0 {
                return false;
            }
            let signed = spec.encoding.starts_with('s') && bytes * 8 < size;
            match constant(a) {
                None => bytes >= 4,
                Some(n) => {
                    n >= -(1i128 << (bytes * 8 - 1))
                        && n < (1i128 << (bytes * 8 - u16::from(signed)))
                }
            }
        }
    }
}

struct Choice<'a> {
    form: &'static IsaForm,
    size: u16,
    p66: bool,
    w: bool,
    operands: SmallVec<[Option<&'a Operand>; 4]>,
}

fn match_form<'a>(
    form: &'static IsaForm,
    i: &'a Instruction,
    d: &Dialect,
    short: bool,
    size: u16,
    p66: bool,
    w: bool,
    address: u16,
) -> Option<Choice<'a>> {
    if !form.allows_size(size)
        || form.addresses & if address == 32 { 2 } else { 4 } == 0
        || d.size.is_some_and(|n| n != size)
        || (form.w >= 0 && (form.w == 1) != w)
        || (i.lock && !form.lock)
        || d.byte.is_some_and(|b| b != form.byte)
    {
        return None;
    }
    if p66 && w && form.prefix != 0x66 {
        return None;
    }
    if w && form.w < 0 && matches!(form.osz, "default64" | "force64") {
        return None;
    }
    if d.group.is_some_and(|g| form.reg_mask != 1 << g) {
        return None;
    }
    if matches!(i.opcode, Nop)
        && !i.operands.is_empty()
        && (form.byte != 0x1f || form.reg_mask != 1)
    {
        return None;
    }
    if matches!(i.opcode, Xchg) && form.mod_mask == 0 {
        return None;
    }
    if matches!(i.opcode, Core(CoreOpcode::XchgAcc)) && form.mod_mask != 0 {
        return None;
    }
    if matches!(i.opcode, Core(CoreOpcode::Ud0 | CoreOpcode::Ud1)) && i.operands.len() != 2 {
        return None;
    }
    let vex = d.name.starts_with('v') && !matches!(d.name.as_str(), "verr" | "verw")
        || matches!(
            i.opcode,
            Andn | Bextr
                | Blsi
                | Blsmsk
                | Blsr
                | Shlx
                | Shrx
                | Sarx
                | Rorx
                | Bzhi
                | Pext
                | Pdep
                | Mulx
        );
    if form.vex != vex {
        return None;
    }
    if matches!(d.name.as_str(), "fnstenv" | "fldenv" | "fnsave" | "frstor") {
        let wanted = if matches!(i.operands.first(),Some(M(m)) if m.width==16) {
            16
        } else {
            32
        };
        if size != wanted {
            return None;
        }
    }
    if form.prefix > 0 && d.repeat != 0 && form.prefix != d.repeat as i16 {
        return None;
    }
    if d.repeat == 0 && form.prefix > 0 && matches!(i.opcode, Core(CoreOpcode::String { .. })) {
        return None;
    }
    if d.repeat != 0
        && form.prefix <= 0
        && !(if d.repeat == 0xf3 {
            form.rep
        } else {
            form.repne
        })
    {
        return None;
    }
    let far = matches!(
        i.opcode,
        Core(CoreOpcode::CallFar | CoreOpcode::JmpFar | CoreOpcode::RetFar { .. })
    );
    if matches!(form.mnemonic, "call" | "jmp" | "ret") && form.far != far {
        return None;
    }
    let rel = form.operands.iter().find(|o| o.kind == "rel");
    if let Some(spec) = rel {
        let required = if matches!(i.opcode, Jmp | Jcc(_)) {
            if short { 1 } else { 4 }
        } else if matches!(i.opcode, Call) {
            4
        } else {
            1
        };
        if spec.bytes(size) != required {
            return None;
        }
    }
    let raw = d.original.as_str();
    if raw == "movabs" || raw == "movabs32" {
        if i.operands.iter().any(|o| matches!(o, M(_))) {
            if !form.operands.iter().any(|s| s.kind == "moffs") {
                return None;
            }
        } else if form
            .operands
            .iter()
            .find(|s| s.kind == "imm")
            .is_none_or(|s| s.bytes(size) != 8)
        {
            return None;
        }
    } else if form.operands.iter().any(|s| s.kind == "moffs") {
        return None;
    }
    if let Some(direction) = d.direction {
        if form
            .operands
            .get(direction)
            .is_none_or(|s| s.encoding != "modrm_reg")
        {
            return None;
        }
    }
    if matches!(i.opcode, Core(CoreOpcode::PushRm | CoreOpcode::PopRm)) && form.mod_mask == 0 {
        return None;
    }
    if matches!(i.opcode, Core(CoreOpcode::MovImm)) && form.mod_mask == 0 {
        return None;
    }
    if matches!(i.opcode, Movq6e | Movq7e) && !w {
        return None;
    }
    if matches!(i.opcode, Core(CoreOpcode::RclImm | CoreOpcode::RcrImm))
        && !form.operands.iter().any(|o| o.kind == "imm")
    {
        return None;
    }
    if matches!(i.opcode, Core(CoreOpcode::Int)) && d.name == "int3" {
        return Some(Choice {
            form,
            size,
            p66,
            w,
            operands: SmallVec::new(),
        });
    }
    let mut operands = SmallVec::new();
    let mut used = 0;
    for (index, spec) in form.operands.iter().enumerate() {
        if matches!(i.opcode, Imul)
            && i.operands.len() == 2
            && matches!(&i.operands[1], I(_))
            && form.operands.len() == 3
            && index == 1
            && spec.encoding == "modrm_rm"
        {
            operands.push(Some(&i.operands[0]));
            continue;
        }
        if matches!(i.operands.get(used),Some(M(m)) if m.width==0)
            && spec.widths.windows(2).any(|w| w[0] != w[1])
            && !i.operands.iter().any(gpr)
            && !matches!(
                i.opcode,
                Push | Pop
                    | Call
                    | Jmp
                    | Core(
                        CoreOpcode::PushRm
                            | CoreOpcode::PopRm
                            | CoreOpcode::Pushw
                            | CoreOpcode::Popw
                    )
            )
        {
            return None;
        }
        if matches!(i.operands.get(used),Some(M(m)) if m.width==0)
            && spec.kind == "mem"
            && matches!(
                i.opcode,
                Mov | Add
                    | Or
                    | Adc
                    | Sbb
                    | And
                    | Sub
                    | Xor
                    | Cmp
                    | Test
                    | Neg
                    | Not
                    | Mul
                    | Imul
                    | Div
                    | Idiv
                    | Inc
                    | Dec
                    | Shl
                    | Shr
                    | Sar
                    | Rol
                    | Ror
                    | Core(
                        CoreOpcode::Rcl | CoreOpcode::Rcr | CoreOpcode::RclImm | CoreOpcode::RcrImm
                    )
            )
            && !i.operands.iter().any(gpr)
        {
            return None;
        }
        let zero_extend = matches!(
            d.name.as_str(),
            "pmovmskb" | "movmskps" | "movmskpd" | "pextrw" | "pinsrw"
        );
        let compatible = zero_extend
            && matches!(spec.kind, "gpr32" | "gpr32_32_64")
            && matches!(i.operands.get(used),Some(R(r)) if r.class==RegisterClass::Gpr&&r.width==64);
        let address_only =
            matches!(d.name.as_str(), "clflush" | "invlpg") || d.name.starts_with("prefetch");
        let compatible = compatible
            || address_only
                && spec.kind == "mem"
                && matches!(i.operands.get(used),Some(M(m)) if matches!(m.width,0|8));
        let compatible = compatible
            || matches!(d.name.as_str(), "fnstenv" | "fldenv" | "fnsave" | "frstor")
                && spec.kind == "mem"
                && matches!(i.operands.get(used),Some(M(m)) if matches!(m.width,0|16|32));
        let compatible = compatible
            || d.name == "lsl"
                && index == 1
                && matches!(i.operands.get(used),Some(R(r)) if r.class==RegisterClass::Gpr&&r.width==size);
        let skip_st0 = spec.kind == "implicit_reg"
            && spec.register == "st0"
            && i.operands.len() < form.operands.len();
        if !skip_st0
            && used < i.operands.len()
            && (compatible || matches_operand(*spec, &i.operands[used], size, address))
        {
            if spec.encoding == "opcode"
                && !matches!(&i.operands[used],R(r) if r.number&7==form.byte&7)
            {
                return None;
            }
            operands.push(Some(&i.operands[used]));
            used += 1;
        } else if spec.kind == "implicit_reg" && spec.register == "st0" && form.extension == "X87" {
            operands.push(None);
        } else if spec.encoding == "modrm_reg"
            && form.reg_mask.count_ones() == 1
            && used == i.operands.len()
        {
            operands.push(None);
        } else if (spec.kind == "implicit_imm1" && used == i.operands.len())
            || (spec.kind == "imm" && d.control.is_some() && index + 1 == form.operands.len())
        {
            operands.push(None);
        } else if spec.kind == "fpr"
            && i.operands.is_empty()
            && matches!(
                d.name.as_str(),
                "fxch"
                    | "fucomi"
                    | "fucomip"
                    | "fcom"
                    | "fcomp"
                    | "fucom"
                    | "fucomp"
                    | "faddp"
                    | "fmulp"
                    | "fsubp"
                    | "fsubrp"
                    | "fdivp"
                    | "fdivrp"
            )
        {
            operands.push(None);
        } else {
            return None;
        }
    }
    if matches!(d.name.as_str(), "blendvps" | "blendvpd" | "pblendvb") {
        if used + 1 == i.operands.len()
            && matches!(i.operands.last(),Some(R(r)) if r.class==RegisterClass::Xmm&&r.number==0)
        {
            used += 1;
        } else if used != i.operands.len() {
            return None;
        }
    }
    if used != i.operands.len() {
        return None;
    }
    if form.gather {
        let registers: SmallVec<[u8; 3]> = i
            .operands
            .iter()
            .filter_map(|o| match o {
                R(r) => Some(r.number),
                M(m) => m.index.map(|r| r.number),
                _ => None,
            })
            .collect();
        if registers
            .iter()
            .enumerate()
            .any(|(n, r)| registers[n + 1..].contains(r))
        {
            return None;
        }
    }
    Some(Choice {
        form,
        size,
        p66,
        w,
        operands,
    })
}

fn emit<'a>(
    choice: &Choice<'a>,
    i: &'a Instruction,
    d: &Dialect,
    address: u16,
) -> Result<Encoding<'a>, String> {
    let f = choice.form;
    let mut e = Encoding::default();
    if d.wait {
        e.bytes.push(0x9b);
    }
    if i.lock {
        e.bytes.push(0xf0);
    }
    let segment = d.segment.or_else(|| {
        f.operands.iter().zip(&choice.operands).find_map(|(s, o)| {
            if s.kind == "moffs" {
                if let Some(M(m)) = o {
                    return m.segment;
                }
            }
            None
        })
    });
    if let Some(segment) = segment {
        e.bytes.push(segment);
    }
    let mut field = f.reg_mask.trailing_zeros() as u8;
    let mut rm = None;
    let mut rm_number = f.rm_mask.trailing_zeros() as u8;
    let mut opcode_reg = 0;
    let mut source = None;
    let mut field_byte = None;
    for (spec, op) in f.operands.iter().zip(&choice.operands) {
        let r = match op {
            Some(R(r)) => Some(*r),
            _ => None,
        };
        let number = r.map_or(if spec.kind == "fpr" { 1 } else { 0 }, |r| r.number);
        match spec.encoding {
            "modrm_reg" => {
                field = number;
                field_byte = r;
            }
            "modrm_rm" => {
                rm = *op;
                rm_number = number;
            }
            "opcode" => opcode_reg = number,
            "ndsndd" => source = Some(number),
            _ => {}
        }
    }
    let mut prefix = if f.prefix > 0 { f.prefix as u8 } else { 0 };
    if d.repeat != 0 {
        prefix = d.repeat;
    }
    let bits = if choice.w {
        64
    } else if choice.p66 && prefix != 0x66 {
        16
    } else {
        32
    };
    let (mut code, n) = f.opcode();
    code[n - 1] |= opcode_reg & 7;
    if f.vex {
        if d.address32 {
            return Err("32-bit addresses are not supported on VEX forms".into());
        }
        let pp = match prefix {
            0x66 => 1,
            0xf3 => 2,
            0xf2 => 3,
            _ => 0,
        };
        if let Some(rm) = rm {
            e.vex_rm_bits(
                f.map,
                pp,
                f.byte,
                field,
                source,
                rm,
                f.vector == 256,
                choice.w,
            )?;
        } else {
            e.bytes.extend([
                0xc5,
                0x80 | source.map_or(0x78, |r| (!r & 15) << 3)
                    | if f.vector == 256 { 4 } else { 0 }
                    | pp,
                f.byte,
            ]);
        }
    } else if let Some(rm) = rm {
        e.rm(prefix, &code[..n], bits, field, rm, field_byte)?;
    } else {
        if address == 32 {
            e.bytes.push(0x67);
        }
        if prefix != 0 {
            e.bytes.push(prefix);
        }
        if choice.p66 && prefix != 0x66 {
            e.bytes.push(0x66);
        }
        let r = opcode_reg >> 3;
        let force = f
            .operands
            .iter()
            .zip(&choice.operands)
            .any(|(_, o)| matches!(o,Some(R(r)) if r.width==8&&r.number>=4&&!r.high_byte));
        let rex = 0x40 | if choice.w { 8 } else { 0 } | r;
        if rex != 0x40 || force {
            e.bytes.push(rex);
        }
        e.bytes.extend_from_slice(&code[..n]);
        if f.mod_mask != 0 {
            e.bytes.push(0xc0 | (field << 3) | rm_number);
        }
    }
    for (spec, op) in f.operands.iter().zip(&choice.operands) {
        if spec.kind == "rel" {
            let Some(I(a)) = op else {
                return Err("branch requires target".into());
            };
            e.branch(
                a,
                matches!(i.opcode, Call | Jmp),
                spec.bytes(choice.size) as u8,
            )?;
        } else if spec.kind == "imm" {
            if let Some(I(a)) = op {
                e.imm(
                    a,
                    spec.bytes(choice.size) as u8,
                    spec.encoding.starts_with('s') && (spec.bytes(choice.size) * 8 < choice.size),
                )?;
            } else {
                e.number(
                    d.control.unwrap_or(0) as i128,
                    spec.bytes(choice.size) as u8,
                );
            }
        } else if spec.encoding == "is4" {
            let Some(R(r)) = op else {
                return Err("is4 requires register".into());
            };
            e.bytes.push(r.number << 4);
        } else if spec.kind == "moffs" {
            let Some(M(m)) = op else {
                return Err("moffs requires memory".into());
            };
            e.imm(&m.displacement, (address / 8) as u8, false)?;
        }
    }
    if e.bytes.len() > 15 {
        return Err("instruction exceeds 15 bytes".into());
    }
    // LLVM/GAS canonicalize operand size before F2/F3, and LOCK after the
    // segment/address/operand-size prefixes. Fixup offsets are unchanged.
    if prefix == 0xf2 || prefix == 0xf3 {
        if let Some(p) = e.bytes.windows(2).position(|p| p == [prefix, 0x66]) {
            e.bytes.swap(p, p + 1);
        }
    }
    if i.lock {
        let end = 1 + e.bytes[1..]
            .iter()
            .take_while(|b| matches!(b, 0x64 | 0x65 | 0x66 | 0x67))
            .count();
        e.bytes[..end].rotate_left(1);
    }
    let len = e.bytes.len();
    for fix in &mut e.fixups {
        if matches!(fix.kind, 2 | 4 | 9 | 15 | 22) {
            fix.adjust -= (len - fix.offset) as i128;
        }
    }
    Ok(e)
}

pub(super) fn encode(i: &Instruction, short: bool) -> Result<Encoding<'_>, String> {
    let d = dialect(i);
    if matches!(i.opcode, Core(CoreOpcode::PushRm | CoreOpcode::PopRm))
        && i.operands.iter().any(|o| matches!(o,M(m) if m.width==0))
    {
        return Err("exact r/m stack form needs a width".into());
    }
    if d.control.is_some() && i.operands.len() != 2 {
        return Err("comparison alias requires exactly two operands".into());
    }
    if matches!(i.opcode, Core(_))
        && i.operands
            .iter()
            .any(|o| matches!(o,M(m) if m.displacement.modifier==Modifier::GotTpOff))
    {
        return Err("GOTTPOFF is not valid on core alternate forms".into());
    }
    if let Core(CoreOpcode::String { kind, segment, .. }) = i.opcode {
        use keel_asm_ir::x86::StringKind;
        if segment.is_some()
            && !matches!(
                kind,
                StringKind::Movs | StringKind::Lods | StringKind::Cmps | StringKind::Outs
            )
        {
            return Err("string form does not accept segment override".into());
        }
    }
    if d.segment.is_some_and(|s| !matches!(s, 0x64 | 0x65)) {
        return Err("invalid segment override".into());
    }
    if let Core(CoreOpcode::Loop { .. }) = i.opcode {
        if !matches!(i.operands.as_slice(),[I(a)] if a.expression.add.is_some()&&a.expression.subtract.is_none()&&a.modifier==Modifier::None)
        {
            return Err("loop requires a plain symbol target".into());
        }
    }
    if matches!(i.opcode, Mov)
        && matches!(i.operands.first(),Some(R(r)) if r.class==RegisterClass::Segment&&r.number==1)
    {
        return Err("MOV cannot load CS".into());
    }
    if matches!(i.opcode, V2(keel_asm_ir::x86::V2Opcode::Crc32))
        && i.operands.iter().any(|o| matches!(o,M(m) if m.width==0))
    {
        return Err("CRC32 memory operand needs a width".into());
    }
    if matches!(i.opcode, Fldcw | Fnstcw)
        && i.operands.iter().any(|o| matches!(o,M(m) if m.width==0))
    {
        return Err("x87 control word needs explicit word width".into());
    }
    if matches!(
        i.opcode,
        V2(keel_asm_ir::x86::V2Opcode::Fisttp) | Fld | Fstp | Fild | Fistp | Fadd | Fmul | X87(_)
    ) && i.operands.iter().any(|o| matches!(o,M(m) if m.width==0))
        && !matches!(
            d.name.as_str(),
            "fnstenv" | "fldenv" | "fnsave" | "frstor" | "fnstsw" | "fnstcw" | "fbld" | "fbstp"
        )
    {
        return Err("x87 memory operand needs a width".into());
    }
    let address = if d.address32
        || i.operands
            .iter()
            .any(|o| matches!(o,M(m) if m.base.into_iter().chain(m.index).any(|r|r.width==32)))
    {
        32
    } else {
        64
    };
    let mut best: Option<((usize, u16, bool), Encoding<'_>)> = None;
    let has16 = d.size == Some(16) || i.operands.iter().any(|o| width(o) == 16);
    let has64 = d.size == Some(64) || i.operands.iter().any(|o| width(o) == 64);
    if i.operands.iter().any(|o|matches!(o,I(a) if constant(a).is_some_and(|n|n<0)))
        && (i.opcode.required_features().avx||i.opcode.required_features().bmi2||matches!(i.opcode,V2(_)|Core(CoreOpcode::RetFar{..}|CoreOpcode::Rcl|CoreOpcode::Rcr|CoreOpcode::RclImm|CoreOpcode::RcrImm|CoreOpcode::Enter{..}|CoreOpcode::Int|CoreOpcode::IntImm|CoreOpcode::In|CoreOpcode::Out))
            || i.operands.iter().any(|o|matches!(o,R(r) if matches!(r.class,RegisterClass::Xmm|RegisterClass::Mmx|RegisterClass::Ymm)))){return Err("control requires an unsigned immediate".into());}
    for f in isa::forms(&d.name) {
        for p66 in [false, true] {
            for w in [false, true] {
                if p66 && !has16 && f.sizes != 1 {
                    continue;
                }
                if w && !has64 && f.w != 1 && f.sizes != 4 {
                    continue;
                }
                let size = f.operand_size(p66, w);
                let Some(mut choice) = match_form(f, i, &d, short, size, p66, w, address) else {
                    continue;
                };
                if matches!(i.opcode, Vandps | Vandpd | Vorps | Vorpd | Vxorps | Vxorpd) {
                    let src = f.operands.iter().position(|o| o.encoding == "ndsndd");
                    let rm = f.operands.iter().position(|o| o.encoding == "modrm_rm");
                    if let (Some(src), Some(rm)) = (src, rm) {
                        if matches!((choice.operands[src],choice.operands[rm]),(Some(R(a)),Some(R(b))) if a.number<8&&b.number>=8)
                        {
                            choice.operands.swap(src, rm);
                        }
                    }
                }
                let Ok(encoded) = emit(&choice, i, &d, address) else {
                    continue;
                };
                let immediate = f
                    .operands
                    .iter()
                    .filter(|o| o.kind == "imm")
                    .map(|o| o.bytes(size))
                    .sum();
                let score = (encoded.bytes.len(), immediate, w);
                if best.as_ref().is_none_or(|(old, _)| score < *old) {
                    best = Some((score, encoded));
                }
            }
        }
    }
    best.map(|(_, e)| e).ok_or_else(|| {
        format!(
            "unsupported generated operands for {:?}: {:?}",
            i.opcode, i.operands
        )
    })
}

pub(super) fn vector_or_state(i: &Instruction) -> bool {
    let d = dialect(i);
    isa::forms(&d.name).iter().any(|f| {
        f.operands
            .iter()
            .any(|o| matches!(o.kind, "xmm" | "ymm" | "mmx"))
            || matches!(f.extension, "SSE" | "SSE2" | "FXSAVE")
            || f.mnemonic.starts_with("fxsave")
            || f.mnemonic.starts_with("fxrstor")
    })
}
