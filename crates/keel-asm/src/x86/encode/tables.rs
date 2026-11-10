//! Operand matching and emission driven by the shared generated ISA forms.
use super::*;
use keel_asm_ir::x86::{
    CoreOpcode, Repeat,
    isa::{IsaForm, IsaOperand, OperandSizeRule},
};
use smallvec::SmallVec;

/// A mnemonic, built in place: every instruction encoded needs one.
#[derive(Clone, Debug, Default)]
struct Name(SmallVec<[u8; 24]>);
impl Name {
    fn new(parts: &[&str]) -> Self {
        let mut name = Self::default();
        for part in parts {
            name.0.extend_from_slice(part.as_bytes());
        }
        name
    }
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("mnemonics are built from whole strings")
    }
    /// Keep the first `len` bytes. Mnemonics are ASCII.
    fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }
}
impl std::ops::Deref for Name {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_slice() == other.as_bytes()
    }
}
impl std::fmt::Write for Name {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

struct Dialect {
    name: Name,
    /// `movabs`: a 64-bit immediate or absolute address form.
    movabs: bool,
    byte: Option<u8>,
    direction: Option<usize>,
    size: Option<u16>,
    control: Option<u8>,
    wait: bool,
    address32: bool,
    repeat: u8,
    segment: Option<u8>,
    group: Option<u8>,
    /// Whether the instruction needs APX (REX2) whatever the form.
    apx: bool,
    /// Whether the instruction selects a VEX form.
    vex: bool,
}

fn dialect(i: &Instruction) -> Dialect {
    let source = if let Core(op) = i.opcode {
        Name::new(&[op.mnemonic()])
    } else {
        let mut name = Name::default();
        std::fmt::Write::write_fmt(&mut name, format_args!("{}", i.opcode))
            .expect("writing a mnemonic cannot fail");
        name
    };
    let raw = source.as_str();
    let mut d = Dialect {
        name: source.clone(),
        movabs: false,
        byte: None,
        direction: None,
        size: None,
        control: None,
        wait: false,
        address32: false,
        repeat: 0,
        segment: None,
        group: None,
        apx: false,
        vex: false,
    };
    if let Core(op) = i.opcode {
        match op {
            CoreOpcode::String {
                repeat,
                address32,
                segment,
                ..
            } => {
                d.name = Name::new(&[op.mnemonic()]);
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
                d.name = Name::new(&["ret"]);
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
                    d.name = Name::new(&["int3"]);
                }
            }
            _ => {}
        }
    }
    if let Fixed(op) = i.opcode {
        let (name, size, wait) = op.source();
        d.name = Name::new(&[name]);
        d.size = size;
        d.wait = wait;
    }
    if let Some((base, suffix)) = raw.split_once('.') {
        d.name = Name::new(&[base]);
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
        d.name = Name::new(&["mov"]);
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
        && matches!(raw, "fstcw" | "fstsw" | "fstenv" | "fsave")
    {
        d.name = Name::new(&["fn", &raw[1..]]);
        d.wait = true;
    }
    if matches!(
        raw,
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
        d.name = Name::new(&["cmp", tag]);
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
    let alias = match d.name.as_str() {
        "callf" => Some("call"),
        "jmpf" => Some("jmp"),
        "xlatb" => Some("xlat"),
        "sal" => Some("shl"),
        _ => None,
    };
    if let Some(alias) = alias {
        d.name = Name::new(&[alias]);
    }
    for stem in ["cmov", "set", "j"] {
        if let Some(cc) = d.name.strip_prefix(stem) {
            let canonical = keel_asm_ir::x86::isa::parse_condition_code(cc)
                .and_then(keel_asm_ir::x86::isa::isa_condition_code_name)
                .unwrap_or(cc);
            d.name = Name::new(&[stem, canonical]);
            break;
        }
    }
    d.movabs = raw == "movabs" || raw == "movabs32";
    d.apx = i.required_features().apx_f;
    d.vex = i.opcode.required_features().amx_tile
        || d.name.starts_with('v') && !matches!(d.name.as_str(), "verr" | "verw")
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
            if !matches!(spec.kind, "imm" | "abs") {
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
                    let n = if signed && size == 64 {
                        signed_word64(n)
                    } else {
                        n
                    };
                    full_byte_pattern(n, bytes, size)
                        || (n >= -(1i128 << (bytes * 8 - 1))
                            && n < (1i128 << (bytes * 8 - u16::from(signed))))
                }
            }
        }
    }
}

// A full-width byte immediate is an eight-bit pattern. GNU accepts negative
// spellings down to -256; sign-extending immediates of wider operations keep
// their strict signed range instead.
fn full_byte_pattern(n: i128, bytes: u16, size: u16) -> bool {
    bytes == 1 && size == 8 && (-256..=255).contains(&n)
}

const REGISTER: u8 = 1;
const MEMORY: u8 = 2;
const IMMEDIATE: u8 = 4;
/// The operand classes a form operand can take, as [`matches_operand`] and
/// the compatible spellings in [`match_form`] accept them.
fn accepts(spec: IsaOperand) -> u8 {
    match spec.kind {
        "mem" | "mem_vsibx" | "mem_vsiby" | "agen" | "agen_norel" | "moffs" => MEMORY,
        "imm" | "abs" | "rel" | "implicit_imm1" => IMMEDIATE,
        "implicit_reg" => REGISTER,
        kind if RegisterClass::from_isa_kind(kind).is_some() => REGISTER,
        _ => REGISTER | MEMORY | IMMEDIATE,
    }
}
fn class(o: &Operand) -> u8 {
    match o {
        R(_) => REGISTER,
        M(_) => MEMORY,
        I(_) => IMMEDIATE,
    }
}
/// Whether `form` can take operands of these classes in order. [`match_form`]
/// gives each operand, in order, to a form operand that accepts its class, so
/// a form failing this cannot match. Mnemonics with many forms, such as
/// `mov`, reject most of them here.
fn may_match(form: &IsaForm, classes: &[u8]) -> bool {
    let mut next = 0;
    for &spec in form.operands {
        if classes.get(next).is_some_and(|&c| accepts(spec) & c != 0) {
            next += 1;
        }
    }
    next == classes.len()
}

struct Choice<'a> {
    form: &'static IsaForm,
    size: u16,
    p66: bool,
    w: bool,
    operands: SmallVec<[Option<&'a Operand>; 4]>,
}

struct FormContext {
    short: bool,
    size: u16,
    p66: bool,
    w: bool,
    address: u16,
}

fn match_form<'a>(
    form: &'static IsaForm,
    i: &'a Instruction,
    d: &Dialect,
    name: &str,
    context: FormContext,
) -> Option<Choice<'a>> {
    let FormContext {
        short,
        size,
        p66,
        w,
        address,
    } = context;
    let rex2 = form.rex2 == 1 || d.apx;
    if rex2 && (form.rex2 == -1 || form.vex || form.map > 1) {
        return None;
    }
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
    if w && form.w < 0
        && matches!(
            form.osz,
            OperandSizeRule::Default64 | OperandSizeRule::Force64
        )
    {
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
    if form.vex != d.vex {
        return None;
    }
    if matches!(name, "fnstenv" | "fldenv" | "fnsave" | "frstor") {
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
    if d.movabs {
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
    if let Some(direction) = d.direction
        && form
            .operands
            .get(direction)
            .is_none_or(|s| s.encoding != "modrm_reg")
    {
        return None;
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
    if matches!(i.opcode, Core(CoreOpcode::Int)) && name == "int3" {
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
            name,
            "pmovmskb" | "movmskps" | "movmskpd" | "pextrw" | "pinsrw"
        );
        let compatible = zero_extend
            && matches!(spec.kind, "gpr32" | "gpr32_32_64")
            && matches!(i.operands.get(used),Some(R(r)) if r.class==RegisterClass::Gpr&&r.width==64);
        let address_only = matches!(name, "clflush" | "invlpg") || name.starts_with("prefetch");
        let compatible = compatible
            || address_only
                && spec.kind == "mem"
                && matches!(i.operands.get(used),Some(M(m)) if matches!(m.width,0|8));
        let compatible = compatible
            || matches!(name, "fnstenv" | "fldenv" | "fnsave" | "frstor")
                && spec.kind == "mem"
                && matches!(i.operands.get(used),Some(M(m)) if matches!(m.width,0|16|32));
        // Pinned Zydis/XED describe the decoded register view (GPRv for
        // LAR, GPRz for LSL). GNU/Clang also accept a 16-bit selector
        // spelling; only the destination selects the encoded operand size.
        let compatible = compatible
            || matches!(name, "lar" | "lsl")
                && index == 1
                && spec.encoding == "modrm_rm"
                && matches!(spec.kind, "gpr16_32_64" | "gpr16_32_32")
                && matches!(i.operands.get(used),Some(R(r)) if r.class==RegisterClass::Gpr && (r.width==16 || r.width==size));
        let compatible = compatible
            || name == "mov"
                && form.byte == 0x8e
                && spec.kind == "gpr16"
                && matches!(i.operands.get(used),Some(R(r)) if r.class==RegisterClass::Gpr && r.width==32);
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
        } else if (spec.kind == "implicit_reg" && spec.register == "st0" && form.extension == "X87")
            || (spec.encoding == "modrm_reg"
                && form.reg_mask.count_ones() == 1
                && used == i.operands.len())
            || (spec.kind == "implicit_imm1" && used == i.operands.len())
            || (spec.kind == "imm" && d.control.is_some() && index + 1 == form.operands.len())
            || (spec.kind == "fpr"
                && i.operands.is_empty()
                && matches!(
                    name,
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
                ))
        {
            operands.push(None);
        } else {
            return None;
        }
    }
    if matches!(name, "blendvps" | "blendvpd" | "pblendvb") {
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
    if form.isa.starts_with("AMX_")
        && form.rm_mask == 1 << 4
        && i.operands
            .iter()
            .any(|o| matches!(o, M(m) if m.base.is_some_and(|r| r.class == RegisterClass::Rip)))
    {
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
            if s.kind == "moffs"
                && let Some(M(m)) = o
            {
                return m.segment;
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
        if d.address32 && !f.isa.starts_with("AMX_") {
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
                VexPrefix {
                    map: f.map,
                    pp,
                    field,
                    source,
                    vector256: f.vector == 256,
                    wide: choice.w,
                    force_sib: f.rm_mask == 1 << 4,
                },
                f.byte,
                rm,
                address == 32,
            )?;
        } else {
            let v =
                source.map_or(0x78, |r| (!r & 15) << 3) | if f.vector == 256 { 4 } else { 0 } | pp;
            if f.map == 1 && !choice.w {
                e.bytes.extend([0xc5, 0x80 | v, f.byte]);
            } else {
                e.bytes.extend([
                    0xc4,
                    0xe0 | f.map,
                    v | if choice.w { 0x80 } else { 0 },
                    f.byte,
                ]);
            }
            if f.mod_mask != 0 {
                e.bytes.push(0xc0 | (field << 3) | rm_number);
            }
        }
    } else if let Some(rm) = rm {
        e.rm(
            LegacyPrefix {
                mandatory: prefix,
                bits,
                rex2: f.rex2 == 1 || d.apx,
            },
            &code[..n],
            field,
            rm,
            field_byte,
        )?;
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
        let rex = 0x40 | if choice.w { 8 } else { 0 } | (r & 1);
        if f.rex2 == 1 || d.apx {
            e.bytes
                .extend([0xd5, (f.map << 7) | ((r >> 1) << 4) | (rex & 15)]);
            e.bytes.extend_from_slice(&code[usize::from(f.map == 1)..n]);
        } else {
            if rex != 0x40 || force {
                e.bytes.push(rex);
            }
            e.bytes.extend_from_slice(&code[..n]);
        }
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
        } else if matches!(spec.kind, "imm" | "abs") {
            if let Some(I(a)) = op {
                if let Some(n) = constant(a)
                    && full_byte_pattern(n, spec.bytes(choice.size), choice.size)
                {
                    e.operand_constant(a, n, 1);
                } else {
                    e.imm(
                        a,
                        spec.bytes(choice.size) as u8,
                        spec.encoding.starts_with('s')
                            && (spec.bytes(choice.size) * 8 < choice.size),
                    )?;
                }
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
    // Only the leading prefix bytes are searched: ModRM, displacement and
    // immediate bytes can spell the same pair.
    let prefixes = (e.bytes.iter())
        .take_while(|&&b| matches!(b, 0x9b | 0xf0 | 0xf2 | 0xf3 | 0x64..=0x67))
        .count();
    if (prefix == 0xf2 || prefix == 0xf3)
        && let Some(p) = e.bytes[..prefixes]
            .windows(2)
            .position(|p| p == [prefix, 0x66])
    {
        e.bytes.swap(p, p + 1);
    }
    if i.lock {
        let end = 1 + e.bytes[1..]
            .iter()
            .take_while(|b| matches!(b, 0x64..=0x67))
            .count();
        e.bytes[..end].rotate_left(1);
    }
    e.finish()?;
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
    if let Core(CoreOpcode::Loop { .. }) = i.opcode
        && !matches!(i.operands.as_slice(),[I(a)] if a.expression.add.is_some()&&a.expression.subtract.is_none()&&a.modifier==Modifier::None)
    {
        return Err("loop requires a plain symbol target".into());
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
    // Validate the name once; form matching compares it many times.
    let name = d.name.as_str();
    // `int3` matches before its operands, and the blend forms' implicit
    // XMM0 operand has no form operand, so neither can be filtered.
    let classes: SmallVec<[u8; 4]> = i.operands.iter().map(class).collect();
    let filter = !matches!(i.opcode, Core(CoreOpcode::Int))
        && !matches!(name, "blendvps" | "blendvpd" | "pblendvb");
    for f in mnemonics::forms(i.opcode, name) {
        if filter && !may_match(f, &classes) {
            continue;
        }
        for p66 in [false, true] {
            for w in [false, true] {
                if p66 && !has16 && f.sizes != 1 {
                    continue;
                }
                if w && !has64 && f.w != 1 && f.sizes != 4 {
                    continue;
                }
                let size = f.operand_size(p66, w);
                let Some(mut choice) = match_form(
                    f,
                    i,
                    &d,
                    name,
                    FormContext {
                        short,
                        size,
                        p66,
                        w,
                        address,
                    },
                ) else {
                    continue;
                };
                if matches!(i.opcode, Vandps | Vandpd | Vorps | Vorpd | Vxorps | Vxorpd) {
                    let src = f.operands.iter().position(|o| o.encoding == "ndsndd");
                    let rm = f.operands.iter().position(|o| o.encoding == "modrm_rm");
                    if let (Some(src), Some(rm)) = (src, rm)
                        && matches!((choice.operands[src],choice.operands[rm]),(Some(R(a)),Some(R(b))) if a.number<8&&b.number>=8)
                    {
                        choice.operands.swap(src, rm);
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
    mnemonics::forms(i.opcode, d.name.as_str()).iter().any(|f| {
        f.operands
            .iter()
            .any(|o| matches!(o.kind, "xmm" | "ymm" | "mmx"))
            || matches!(f.extension, "SSE" | "SSE2" | "FXSAVE")
            || f.isa.starts_with("AMX_")
            || f.mnemonic.starts_with("fxsave")
            || f.mnemonic.starts_with("fxrstor")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_asm_ir::x86::isa;

    fn check_group(instruction: &Instruction) {
        let dialect = dialect(instruction);
        let actual = mnemonics::forms(instruction.opcode, &dialect.name);
        let expected = isa::forms(&dialect.name);
        assert_eq!(actual.len(), expected.len(), "{:?}", instruction.opcode);
        assert_eq!(
            actual.as_ptr(),
            expected.as_ptr(),
            "{:?}",
            instruction.opcode
        );
    }

    #[test]
    fn generated_opcode_groups_match_dialect_normalization() {
        for &opcode in mnemonics::CASES {
            check_group(&Instruction {
                opcode,
                operands: Vec::new(),
                lock: false,
            });
        }
    }

    #[test]
    fn payload_and_operand_dependent_groups_use_name_lookup() {
        use keel_asm_ir::x86::{LoopKind, StringKind};
        for opcode in [
            Core(CoreOpcode::String {
                kind: StringKind::Movs,
                width: 64,
                repeat: Repeat::Rep,
                address32: false,
                segment: None,
            }),
            Core(CoreOpcode::Loop {
                kind: LoopKind::Zero,
                address32: true,
            }),
            Core(CoreOpcode::RetFar { width: 64 }),
        ] {
            check_group(&Instruction {
                opcode,
                operands: Vec::new(),
                lock: false,
            });
        }
        for (value, name) in [(3, "int3"), (128, "int")] {
            let instruction = Instruction {
                opcode: Core(CoreOpcode::Int),
                operands: vec![I(Address {
                    expression: keel_asm_ir::Expression::constant(value),
                    ..Default::default()
                })],
                lock: false,
            };
            assert_eq!(dialect(&instruction).name, name);
            check_group(&instruction);
        }
    }
}
