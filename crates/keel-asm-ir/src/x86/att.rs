//! AT&T source grammar. Operands are constructed directly in the shared model;
//! no source rewriting or assembler subprocess is involved.
use super::*;

/// Initial source dialect. Syntax directives can change it within a fragment.
/// Intel remains the default for compatibility with existing callers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Syntax {
    #[default]
    Intel,
    Att,
}

/// Stateful source parser, including the register-prefix lexical mode.
#[derive(Clone, Debug)]
pub struct SourceParser {
    syntax: Syntax,
    prefix: bool,
}
impl Default for SourceParser {
    fn default() -> Self {
        Self::new(Syntax::Intel)
    }
}
impl SourceParser {
    pub fn new(syntax: Syntax) -> Self {
        Self {
            syntax,
            prefix: syntax == Syntax::Att,
        }
    }
    pub(super) fn syntax_directive(&mut self, s: &str) -> Result<bool, String> {
        let (name, args) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
        let syntax = match name {
            ".att_syntax" => Syntax::Att,
            ".intel_syntax" => Syntax::Intel,
            _ => return Ok(false),
        };
        let prefix = match args.trim() {
            "prefix" => true,
            "noprefix" => false,
            "" => syntax == Syntax::Att,
            _ => return Err(format!("{name} expects prefix or noprefix")),
        };
        self.syntax = syntax;
        self.prefix = prefix;
        Ok(true)
    }
    pub(super) fn instruction(&self, s: &str) -> Result<Instruction, String> {
        match self.syntax {
            Syntax::Intel => Instruction::parse_intel_prefix(s, self.prefix),
            Syntax::Att => parse(s, self.prefix),
        }
    }
}
impl Instruction {
    /// Parse one AT&T instruction with `%`-prefixed registers.
    pub fn parse_att(s: &str) -> Result<Self, String> {
        parse(s, true)
    }
}

fn register(s: &str, prefix: bool) -> Option<Register> {
    intel_register(s.trim(), prefix)
}

/// Commas within base/index/scale tuples are not operand separators.
fn operands(s: &str) -> Result<Vec<&str>, String> {
    let (mut quoted, mut escaped, mut depth, mut start) = (false, false, 0, 0);
    let mut out = Vec::new();
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted;
        }
        if !quoted {
            match c {
                '(' => {
                    depth += 1;
                    if depth > 1 {
                        return Err("nested AT&T address parentheses".into());
                    }
                }
                ')' => {
                    depth -= 1;
                    if depth < 0 {
                        return Err("unexpected address closing parenthesis".into());
                    }
                }
                ',' if depth == 0 => {
                    out.push(s[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
    }
    if quoted || depth != 0 {
        return Err("unclosed AT&T operand".into());
    }
    out.push(s[start..].trim());
    if out.iter().any(|s| s.is_empty()) {
        return Err("empty AT&T operand".into());
    }
    Ok(out)
}

// The shared linear-expression helper does not represent unary sign chains.
// Reject them here rather than silently changing a relocation addend.
fn expression(s: &str) -> Result<Address, String> {
    let (mut quote, mut escaped, mut sign) = (false, false, false);
    for c in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote && c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            quote = !quote;
            sign = false;
            continue;
        }
        if !quote && !c.is_whitespace() {
            if c == '+' || c == '-' {
                if sign {
                    return Err("consecutive expression signs are not supported".into());
                }
                sign = true;
            } else {
                sign = false;
            }
        }
    }
    address(s)
}

fn memory(s: &str, prefix: bool) -> Result<Operand, String> {
    let (mut s, mut segment) = (s.trim(), None);
    for (name, byte) in if prefix {
        [("%fs:", 0x64), ("%gs:", 0x65)]
    } else {
        [("fs:", 0x64), ("gs:", 0x65)]
    } {
        if let Some(rest) = s.strip_prefix(name) {
            segment = Some(byte);
            s = rest.trim();
            break;
        }
    }
    let mut m = Memory {
        width: 0,
        base: None,
        index: None,
        scale: 1,
        segment,
        displacement: Address::default(),
    };
    // A quoted symbol may contain parentheses; only an unquoted '(' opens a tuple.
    let (mut quote, mut escaped, mut tuple) = (false, false, None);
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote && c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            quote = !quote;
        }
        if c == '(' && !quote {
            tuple = Some(i);
            break;
        }
    }
    if let Some(open) = tuple {
        let body = s[open + 1..]
            .strip_suffix(')')
            .ok_or("unclosed AT&T address")?;
        let fields: Vec<_> = body.split(',').map(str::trim).collect();
        if fields.len() > 3 {
            return Err("AT&T address expects base,index,scale".into());
        }
        if !fields[0].is_empty() {
            m.base = Some(register(fields[0], prefix).ok_or("invalid AT&T base register")?);
        }
        if let Some(index) = fields.get(1).filter(|s| !s.is_empty()) {
            m.index = Some(register(index, prefix).ok_or("invalid AT&T index register")?);
        }
        if let Some(scale) = fields.get(2) {
            let scale = integer(scale)?;
            if ![1, 2, 4, 8].contains(&scale) || m.index.is_none() {
                return Err("AT&T scale requires an index and must be 1, 2, 4 or 8".into());
            }
            m.scale = scale as u8;
        }
        if m.base.is_none() && m.index.is_none() {
            return Err("empty AT&T address tuple".into());
        }
        if !s[..open].trim().is_empty() {
            m.displacement = expression(s[..open].trim())?;
        }
    } else {
        m.displacement = expression(s)?;
    }
    if m.base.is_some_and(|r| {
        !matches!(r.width, 32 | 64) || !matches!(r.class, RegisterClass::Gpr | RegisterClass::Rip)
    }) {
        return Err("addresses require 32-bit or 64-bit general base registers".into());
    }
    if let (Some(base), Some(index)) = (m.base, m.index) {
        if index.class == RegisterClass::Gpr && base.width != index.width {
            return Err("mixed address register widths".into());
        }
    }
    if m.index.is_some_and(|r| {
        !matches!(r.class, RegisterClass::Xmm | RegisterClass::Ymm)
            && (r.class != RegisterClass::Gpr || !matches!(r.width, 32 | 64) || r.number == 4)
    }) || (m.base.is_some_and(|r| r.class == RegisterClass::Rip) && m.index.is_some())
    {
        return Err("invalid address index".into());
    }
    Ok(Operand::Memory(m))
}

fn operand(s: &str, prefix: bool) -> Result<Operand, String> {
    if let Some(s) = s.strip_prefix('$') {
        return Ok(Operand::Immediate(expression(s)?));
    }
    if let Some(r) = register(s, prefix) {
        return Ok(Operand::Register(r));
    }
    memory(s, prefix)
}
fn bits(s: u8) -> Option<u16> {
    Some(match s {
        b'b' => 8,
        b'w' => 16,
        b'l' => 32,
        b'q' => 64,
        _ => return None,
    })
}
fn width(o: &mut Operand, bits: u16) -> Result<(), String> {
    match o {
        Operand::Memory(m) => m.width = bits,
        Operand::Register(r) if r.class != RegisterClass::Gpr || r.width != bits => {
            return Err(format!("AT&T suffix requires a {bits}-bit register"));
        }
        _ => {}
    }
    Ok(())
}
fn scalar(op: Opcode) -> bool {
    if matches!(op, V3(V3Opcode::Lzcnt | V3Opcode::Movbe)) {
        return true;
    }
    use Opcode::*;
    matches!(op, Core(CoreOpcode::Rcl | CoreOpcode::Rcr))
        || matches!(
            op,
            Mov | Movabs
                | Lea
                | Push
                | Pop
                | Ret
                | Leave
                | Nop
                | Add
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
                | Inc
                | Dec
                | Mul
                | Imul
                | Div
                | Idiv
                | Shl
                | Shr
                | Sar
                | Rol
                | Ror
                | Shld
                | Shrd
                | Bsf
                | Bsr
                | Bswap
                | Popcnt
                | Andn
                | Bextr
                | Blsi
                | Blsmsk
                | Blsr
                | Tzcnt
                | Shlx
                | Shrx
                | Sarx
                | Rorx
                | Bzhi
                | Pext
                | Pdep
                | Mulx
                | Bt
                | Bts
                | Btr
                | Btc
                | Xchg
                | Xadd
                | Cmpxchg
                | Call
                | Jmp
                | Cmovcc(_)
                | Setcc(_)
        )
}
fn parse(s: &str, prefix: bool) -> Result<Instruction, String> {
    use Opcode::*;
    let s = s.trim();
    if let Some(i) = super::core::parse_string(s, true, prefix) {
        return i;
    }
    let (first, rest) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
    if first.eq_ignore_ascii_case("data16") {
        let i = parse(rest, prefix)?;
        return if matches!(i.opcode, Core(_)) {
            i.core_prefix("data16")
        } else {
            i.x87_data16()
        };
    }
    if matches!(
        lowercase(first).as_ref(),
        "rep" | "repe" | "repz" | "repne" | "repnz" | "fs" | "gs"
    ) {
        return parse(rest, prefix)?.core_prefix(&lowercase(first));
    }
    if first.eq_ignore_ascii_case("addr32") {
        return parse(rest, prefix)?.core_prefix("addr32");
    }
    if first.eq_ignore_ascii_case("rex64") {
        let mut i = parse(rest, prefix)?;
        i.opcode = match i.opcode {
            V3(v) if !i.lock => V3(v.with_rex_w().ok_or("rex64 requires a PCMP string form")?),
            V2(v) if !i.lock => V2(v
                .with_rex_w()
                .ok_or("rex64 is supported only for PCMP string forms")?),
            _ => return Err("rex64 is supported only for PCMP string forms".into()),
        };
        return Ok(i);
    }
    let (lock, s) = if first.eq_ignore_ascii_case("lock") {
        (true, rest.trim())
    } else {
        (false, s)
    };
    let (mnemonic, args) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
    let name = lowercase(mnemonic);
    let name = name.as_ref();
    let raw = if args.trim().is_empty() {
        Vec::new()
    } else {
        operands(args)?
    };
    // Branch targets have their own grammar: an unadorned expression is direct;
    // '*' is mandatory for a register or memory indirect call/jump.
    let branch = matches!(name, "call" | "callq" | "jmp" | "jmpq")
        || Opcode::parse(name).is_some_and(Opcode::is_direct_branch);
    let far = matches!(
        Opcode::parse(name),
        Some(Core(CoreOpcode::CallFar | CoreOpcode::JmpFar))
    );
    let mut ops = Vec::with_capacity(raw.len());
    for s in raw {
        if far {
            let indirect = s
                .strip_prefix('*')
                .ok_or("AT&T far memory transfer requires `*`")?;
            let memory = operand(indirect.trim(), prefix)?;
            if !matches!(memory, Operand::Memory(_)) {
                return Err("far transfer requires indirect memory".into());
            }
            ops.push(memory);
        } else if branch {
            if let Some(s) = s.strip_prefix('*') {
                if !matches!(name, "call" | "callq" | "jmp" | "jmpq") {
                    return Err("conditional jumps cannot be indirect".into());
                }
                let o = operand(s.trim(), prefix)?;
                if matches!(o, Operand::Immediate(_)) {
                    return Err("indirect branch requires register or memory".into());
                }
                ops.push(o);
            } else {
                if register(s, prefix).is_some() || s.starts_with('$') {
                    return Err("AT&T indirect branch requires `*`".into());
                }
                ops.push(Operand::Immediate(expression(s)?));
            }
        } else {
            ops.push(operand(s, prefix)?);
        }
    }
    if !matches!(Opcode::parse(name), Some(Core(CoreOpcode::Enter { .. }))) {
        ops.reverse();
    }
    if let Some(w) = match name {
        "lcallw" | "ljmpw" => Some(32),
        "lcall" | "ljmp" | "callf" | "jmpf" | "lcalll" | "ljmpl" => Some(48),
        "lcallq" | "ljmpq" => Some(80),
        _ => None,
    } {
        let [Operand::Memory(m)] = ops.as_mut_slice() else {
            return Err("far control transfer requires memory".into());
        };
        m.width = w;
    }
    let invalid = || format!("unsupported AT&T instruction `{mnemonic}`");
    let extension = match name {
        "movsbw" => Some((Movsx, 8, 16)),
        "movsbl" => Some((Movsx, 8, 32)),
        "movsbq" => Some((Movsx, 8, 64)),
        "movswl" => Some((Movsx, 16, 32)),
        "movswq" => Some((Movsx, 16, 64)),
        "movslq" => Some((Movsxd, 32, 64)),
        "movzbw" => Some((Movzx, 8, 16)),
        "movzbl" => Some((Movzx, 8, 32)),
        "movzbq" => Some((Movzx, 8, 64)),
        "movzwl" => Some((Movzx, 16, 32)),
        "movzwq" => Some((Movzx, 16, 64)),
        _ => None,
    };
    let packed_conversion = name
        .strip_suffix('x')
        .map(|s| (s, 128))
        .or_else(|| name.strip_suffix('y').map(|s| (s, 256)))
        .and_then(|(base, w)| {
            Opcode::parse(base)
                .filter(|op| {
                    matches!(
                        op,
                        V3(V3Opcode::Vcvtpd2dq | V3Opcode::Vcvttpd2dq | V3Opcode::Vcvtpd2ps)
                    )
                })
                .map(|op| (op, w))
        });
    let opcode = if let Some((op, w)) = packed_conversion {
        match ops.get_mut(1) {
            Some(Operand::Memory(m)) => m.width = w,
            Some(Operand::Register(r))
                if r.width == w && matches!(r.class, RegisterClass::Xmm | RegisterClass::Ymm) => {}
            _ => return Err("packed conversion suffix disagrees with source".into()),
        }
        op
    } else if let Some(src_bits) = name.strip_prefix("crc32").and_then(|s| {
        if s.len() == 1 {
            bits(s.as_bytes()[0])
        } else {
            None
        }
    }) {
        let [dst, src] = ops.as_mut_slice() else {
            return Err(invalid());
        };
        width(src, src_bits)?;
        if !matches!(dst, Operand::Register(r) if r.class == RegisterClass::Gpr && matches!(r.width, 32 | 64))
        {
            return Err(invalid());
        }
        V2(V2Opcode::Crc32)
    } else if let Some((op, src, dst)) = extension {
        let [a, b] = ops.as_mut_slice() else {
            return Err("AT&T extension expects source and destination".into());
        };
        width(a, dst)?;
        width(b, src)?;
        op
    } else if let Some((op, w)) = X87Opcode::att_memory(name) {
        let [Operand::Memory(m)] = ops.as_mut_slice() else {
            return Err("x87 memory suffix requires one memory operand".into());
        };
        m.width = w;
        Opcode::X87(op)
    } else if let Some((op, w)) = match name {
        "flds" => Some((Fld, 32)),
        "fldl" => Some((Fld, 64)),
        "fldt" => Some((Fld, 80)),
        "fstps" => Some((Fstp, 32)),
        "fstpl" => Some((Fstp, 64)),
        "fstpt" => Some((Fstp, 80)),
        "fadds" => Some((Fadd, 32)),
        "faddl" => Some((Fadd, 64)),
        "fmuls" => Some((Fmul, 32)),
        "fmull" => Some((Fmul, 64)),
        "filds" => Some((Fild, 16)),
        "fildl" => Some((Fild, 32)),
        "fildll" => Some((Fild, 64)),
        "fisttps" => Some((V2(V2Opcode::Fisttp), 16)),
        "fisttpl" => Some((V2(V2Opcode::Fisttp), 32)),
        "fisttpll" => Some((V2(V2Opcode::Fisttp), 64)),
        "fistps" => Some((Fistp, 16)),
        "fistpl" => Some((Fistp, 32)),
        "fistpll" => Some((Fistp, 64)),
        "fldcw" => Some((Fldcw, 16)),
        "fnstcw" => Some((Fnstcw, 16)),
        _ => None,
    } {
        let [Operand::Memory(m)] = ops.as_mut_slice() else {
            return Err(if op == Fld {
                "x87 load requires one memory operand"
            } else {
                "x87 memory form requires one memory operand"
            }
            .into());
        };
        m.width = w;
        op
    } else if name == "fsubp" {
        // GAS/LLVM AT&T reverse the Intel subtraction-pop mnemonics. Keep
        // the shared model in Intel destination/source semantics.
        Fsubrp
    } else if name == "fsubrp" {
        Fsubp
    } else if let Some(op) = match name {
        "cbtw" => Some(Cbw),
        "cwtl" => Some(Cwde),
        "cltq" => Some(Cdqe),
        "cltd" => Some(Cdq),
        "cqto" => Some(Cqo),
        _ => None,
    } {
        op
    } else {
        // Conversion suffixes describe the integer operand, not the XMM value.
        let conversion = name
            .as_bytes()
            .last()
            .and_then(|&b| bits(b))
            .filter(|w| matches!(w, 32 | 64))
            .and_then(|w| {
                Opcode::parse(&name[..name.len() - 1])
                    .filter(|op| {
                        matches!(
                            op,
                            Cvtsi2ss
                                | Cvtsi2sd
                                | Cvttss2si
                                | Cvttsd2si
                                | Cvtss2si
                                | Cvtsd2si
                                | V3(V3Opcode::Vcvtsi2ss
                                    | V3Opcode::Vcvtsi2sd
                                    | V3Opcode::Vcvtss2si
                                    | V3Opcode::Vcvtsd2si
                                    | V3Opcode::Vcvttss2si
                                    | V3Opcode::Vcvttsd2si)
                        )
                    })
                    .map(|op| (op, w))
            });
        if let Some((op, w)) = conversion {
            let index = if matches!(op, Cvtsi2ss | Cvtsi2sd) {
                1
            } else if matches!(op, V3(V3Opcode::Vcvtsi2ss | V3Opcode::Vcvtsi2sd)) {
                2
            } else {
                0
            };
            width(
                ops.get_mut(index)
                    .ok_or("conversion expects two operands")?,
                w,
            )?;
            op
        } else {
            let suffixed = name.as_bytes().last().and_then(|&b| bits(b)).and_then(|w| {
                if matches!(name, "leavew" | "pushw" | "popw") {
                    return None;
                }
                let base = &name[..name.len() - 1];
                let op = if base == "sal" {
                    Some(Shl)
                } else {
                    Opcode::parse(base)
                }?;
                scalar(op).then_some((op, w))
            });
            // movq is also the SIMD transfer mnemonic. The register class makes
            // this unambiguous; a memory-only form is rejected by the encoder.
            let simd_movq = name == "movq"
                && ops
                    .iter()
                    .any(|o| matches!(o, Operand::Register(r) if matches!(r.class, RegisterClass::Xmm | RegisterClass::Mmx)));
            if let Some((op, w)) = suffixed.filter(|_| !simd_movq) {
                if matches!(op, Setcc(_)) && w != 8 {
                    return Err("setcc only supports the byte suffix".into());
                }
                if (op == Nop && ops.is_empty())
                    || matches!(op, Push | Pop)
                        && w != 64
                        && ops.iter().any(|o| matches!(o, Operand::Immediate(_)))
                {
                    return Err(
                        "suffixed form is not representable by the instruction model".into(),
                    );
                }
                if matches!(op, Call | Jmp | Ret | Leave) && w != 64 {
                    return Err("only 64-bit near control transfers are supported".into());
                }
                for (index, o) in ops.iter_mut().enumerate() {
                    if (index == 1
                        && matches!(
                            op,
                            Shl | Shr | Sar | Rol | Ror | Core(CoreOpcode::Rcl | CoreOpcode::Rcr)
                        ))
                        || (index == 2 && matches!(op, Shld | Shrd))
                    {
                        continue;
                    }
                    if op == Mov
                        && matches!(o, Operand::Register(r) if r.class==RegisterClass::Segment)
                    {
                        continue;
                    }
                    width(o, w)?;
                }
                op
            } else {
                if name == "sal" {
                    Shl
                } else {
                    Opcode::parse(name).ok_or_else(invalid)?
                }
            }
        }
    };
    let opcode = match opcode {
        Opcode::X87(op) => Opcode::X87(op.from_att(&ops)),
        other => other,
    };
    // These commutative forms encode the register in ModRM.reg. Canonicalize
    // a memory source into the model's r/m destination position.
    if matches!(opcode, Test | Xchg)
        && matches!(ops.as_slice(), [Operand::Register(_), Operand::Memory(_)])
    {
        ops.swap(0, 1);
    }
    // The current Mov model chooses imm64 for symbolic GPR moves. AT&T movq
    // requires imm32 with R_X86_64_32S; accepting it as imm64 changes link-time
    // overflow semantics. Keep that gap explicit until the model can select it.
    if opcode == Mov
        && matches!(ops.as_slice(), [Operand::Register(r), Operand::Immediate(a)]
        if r.width == 64 && (a.expression.add.is_some() || a.expression.subtract.is_some()))
    {
        return Err("AT&T symbolic movq requires a signed 32-bit immediate relocation, not yet supported by the instruction model".into());
    }
    // AT&T's omitted shift count means one. Normalize to the existing model.
    if matches!(
        opcode,
        Shl | Shr | Sar | Rol | Ror | Core(CoreOpcode::Rcl | CoreOpcode::Rcr)
    ) && ops.len() == 1
    {
        ops.push(Operand::Immediate(Address {
            expression: Expression::constant(1),
            modifier: Modifier::None,
        }));
    }
    // SHLD/SHRD's two-operand aliases take their count from CL.
    if matches!(opcode, Shld | Shrd) && ops.len() == 2 {
        ops.push(Operand::Register(Register::parse("cl").unwrap()));
    }
    // Unsuffixed cvtsi2s{s,d} memory operands default to a 32-bit integer.
    if matches!(opcode, Cvtsi2ss | Cvtsi2sd)
        && let Some(Operand::Memory(m)) = ops.get_mut(1)
        && m.width == 0
    {
        m.width = 32;
    }
    let opcode = opcode.with_operand_form(&ops);
    Instruction {
        opcode,
        operands: ops,
        lock,
    }
    .core_suffix(name)
}
