//! Typed operands used by codegen. Formatting is confined to assembly output;
//! structured emission lowers these values directly to generated encoding IDs.
use keel_asm_ir::{FieldId as F, Instruction, Opcode as O};
use std::fmt;

pub(super) trait Number {
    fn number(self) -> i128;
}
macro_rules! numbers { ($($t:ty),*) => {$ (impl Number for $t { fn number(self)->i128 {self as i128} } impl Number for &$t {fn number(self)->i128{*self as i128}})*}; }
numbers!(u8, u16, u32, u64, usize, i8, i16, i32, i64, i128, isize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Reg {
    pub number: u32,
    pub class: char,
}
impl Reg {
    pub fn scalar(class: impl Prefix, number: impl Number) -> Self {
        Self {
            number: number.number() as u32,
            class: class.prefix(),
        }
    }
    pub fn x(n: impl Number) -> Self {
        Self::scalar('x', n)
    }
    pub fn w(n: impl Number) -> Self {
        Self::scalar('w', n)
    }
    pub fn s(n: impl Number) -> Self {
        Self::scalar('s', n)
    }
    pub fn q(n: impl Number) -> Self {
        Self::scalar('q', n)
    }
    pub fn d(n: impl Number) -> Self {
        Self::scalar('d', n)
    }
    pub fn b(n: impl Number) -> Self {
        Self::scalar('b', n)
    }
    pub fn v8b(n: impl Number) -> Self {
        Self::scalar('v', n)
    }
    pub const SP: Self = Self {
        number: 31,
        class: 'p',
    };
    pub const XZR: Self = Self {
        number: 31,
        class: 'x',
    };
    pub const WZR: Self = Self {
        number: 31,
        class: 'w',
    };
    pub fn bits(self) -> u32 {
        match self.class {
            'q' => 128,
            'w' | 's' => 32,
            'b' => 8,
            _ => 64,
        }
    }
    pub fn fp(self) -> bool {
        matches!(self.class, 's' | 'd' | 'b' | 'v' | 'q')
    }
}
pub(super) trait Prefix {
    fn prefix(self) -> char;
}
impl Prefix for char {
    fn prefix(self) -> char {
        self
    }
}
impl Prefix for &str {
    fn prefix(self) -> char {
        match self {
            "x" => 'x',
            "w" => 'w',
            "s" => 's',
            "d" => 'd',
            _ => panic!("invalid codegen register class"),
        }
    }
}
impl From<&str> for Reg {
    fn from(s: &str) -> Self {
        match s {
            "sp" => Self::SP,
            "x19" => Self::x(19),
            "x29" => Self::x(29),
            "xzr" => Self::XZR,
            "wzr" => Self::WZR,
            _ => panic!("untyped register in codegen: {s}"),
        }
    }
}
impl Reg {
    /// The `Display` text, written without entering `fmt::write`.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, f: &mut W) -> fmt::Result {
        match (self.class, self.number) {
            ('p', _) => f.write_str("sp"),
            ('x', 31) => f.write_str("xzr"),
            ('w', 31) => f.write_str("wzr"),
            ('v', n) => {
                crate::text::write_decimal(f, "v", n.into())?;
                f.write_str(".8b")
            }
            (class, n) => crate::text::write_class(f, class, n.into()),
        }
    }
}
impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Offset {
    Immediate(Option<i128>),
    Index {
        reg: Reg,
        extend: &'static str,
        shift: Option<u32>,
    },
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Address {
    pub base: Reg,
    pub offset: Offset,
    pub pre: bool,
}
impl Address {
    pub fn base(base: impl Into<Reg>) -> Self {
        Self {
            base: base.into(),
            offset: Offset::Immediate(None),
            pre: false,
        }
    }
    pub fn offset(base: impl Into<Reg>, offset: impl Number) -> Self {
        Self {
            base: base.into(),
            offset: Offset::Immediate(Some(offset.number())),
            pre: false,
        }
    }
    pub fn index(base: Reg, reg: Reg, extend: &'static str, shift: Option<u32>) -> Self {
        Self {
            base,
            offset: Offset::Index { reg, extend, shift },
            pre: false,
        }
    }
}
impl Address {
    /// The `Display` text, written without entering `fmt::write`.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, f: &mut W) -> fmt::Result {
        f.write_str("[")?;
        self.base.write_to(f)?;
        match self.offset {
            Offset::Immediate(Some(n)) => crate::text::write_decimal(f, ", #", n)?,
            Offset::Immediate(None) => {}
            Offset::Index { reg, extend, shift } => {
                f.write_str(", ")?;
                reg.write_to(f)?;
                if !extend.is_empty() {
                    f.write_str(", ")?;
                    f.write_str(extend)?;
                }
                if let Some(s) = shift {
                    crate::text::write_decimal(f, " #", s.into())?;
                }
            }
        }
        f.write_str(if self.pre { "]!" } else { "]" })
    }
}
impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Arg {
    Reg(Reg),
    Imm(i128),
    Memory(Address),
    Modifier(&'static str, Option<u32>),
    Cond(u32),
    Barrier(u32),
}
impl Arg {
    pub fn imm(n: impl Number) -> Self {
        Self::Imm(n.number())
    }
    pub fn shift(kind: &'static str, n: impl Number) -> Self {
        Self::Modifier(kind, Some(n.number() as u32))
    }
    pub fn condition(s: &str) -> Self {
        Self::Cond(condition(s))
    }
    pub fn reg(self) -> Reg {
        match self {
            Self::Reg(r) => r,
            _ => panic!("expected typed register"),
        }
    }
    pub fn number(self) -> i128 {
        match self {
            Self::Imm(n) => n,
            _ => panic!("expected typed immediate"),
        }
    }
}
impl From<Reg> for Arg {
    fn from(r: Reg) -> Self {
        Self::Reg(r)
    }
}
impl From<Address> for Arg {
    fn from(a: Address) -> Self {
        Self::Memory(a)
    }
}
impl From<&'static str> for Arg {
    fn from(s: &'static str) -> Self {
        match s {
            "sp" | "x19" | "xzr" | "wzr" => Reg::from(s).into(),
            "uxtb" | "uxth" | "uxtw" | "uxtx" | "sxtb" | "sxth" | "sxtw" | "sxtx" => {
                Self::Modifier(s, None)
            }
            _ => Self::condition(s),
        }
    }
}
pub(super) fn condition(s: &str) -> u32 {
    match s {
        "eq" => 0,
        "ne" => 1,
        "cs" | "hs" => 2,
        "cc" | "lo" => 3,
        "mi" => 4,
        "pl" => 5,
        "vs" => 6,
        "vc" => 7,
        "hi" => 8,
        "ls" => 9,
        "ge" => 10,
        "lt" => 11,
        "gt" => 12,
        "le" => 13,
        "al" => 14,
        "nv" => 15,
        _ => panic!("invalid codegen condition: {s}"),
    }
}
pub(super) fn condition_name(n: u32) -> &'static str {
    [
        "eq", "ne", "hs", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "al",
        "nv",
    ][n as usize]
}
impl Arg {
    /// The `Display` text, written without entering `fmt::write`.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, f: &mut W) -> fmt::Result {
        match self {
            Self::Reg(r) => r.write_to(f),
            Self::Imm(n) => crate::text::write_decimal(f, "#", *n),
            Self::Memory(a) => a.write_to(f),
            Self::Modifier(kind, Some(n)) => {
                f.write_str(kind)?;
                crate::text::write_decimal(f, " #", (*n).into())
            }
            Self::Modifier(kind, None) => f.write_str(kind),
            Self::Cond(n) => f.write_str(condition_name(*n)),
            Self::Barrier(9) => f.write_str("ishld"),
            Self::Barrier(11) => f.write_str("ish"),
            Self::Barrier(n) => crate::text::write_decimal(f, "#", (*n).into()),
        }
    }
}
impl fmt::Display for Arg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}

fn ins(op: &str, tag: &str, form: &str, fields: impl IntoIterator<Item = (F, u32)>) -> Instruction {
    Instruction::new(opcode(op, tag, form), fields)
}
fn tag(r: Reg) -> &'static str {
    match r.class {
        'q' => "Q",
        's' => "S",
        'd' => "D",
        'w' => "32",
        _ => "64",
    }
}
fn flagtag(r: Reg) -> &'static str {
    if r.bits() == 64 { "64S" } else { "32S" }
}
fn modifier(a: Option<&Arg>) -> (&'static str, u32) {
    match a {
        Some(Arg::Modifier(k, n)) => (*k, n.unwrap_or(0)),
        None => ("lsl", 0),
        _ => panic!("expected modifier"),
    }
}
fn extension(s: &str) -> u32 {
    match s {
        "uxtb" => 0,
        "uxth" => 1,
        "uxtw" => 2,
        "uxtx" | "lsl" => 3,
        "sxtb" => 4,
        "sxth" => 5,
        "sxtw" => 6,
        "sxtx" => 7,
        _ => panic!("invalid extension"),
    }
}
fn cc(a: Arg) -> u32 {
    match a {
        Arg::Cond(n) => n,
        _ => panic!("expected condition"),
    }
}
fn zero(r: Reg) -> Arg {
    if r.bits() == 64 { Reg::XZR } else { Reg::WZR }.into()
}
/// Checked logical-immediate plan. `value` is normalized to the execution
/// width; only 32 and 64 are accepted. Work is bounded by six element sizes,
/// with no search over rotations or encoded masks. Zero/all-ones return None.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LogicalImmediate {
    pub value: u64,
    pub n: u32,
    pub immr: u32,
    pub imms: u32,
}

pub(super) fn logical_immediate(value: u64, bits: u32) -> Option<LogicalImmediate> {
    if !matches!(bits, 32 | 64) {
        return None;
    }
    let width_mask = u64::MAX >> (64 - bits);
    let value = value & width_mask;
    for size in [2, 4, 8, 16, 32, 64] {
        if size > bits {
            break;
        }
        let mask = u64::MAX >> (64 - size);
        let element = value & mask;
        let ones = element.count_ones();
        if ones == 0 || ones == size || element * (width_mask / mask) != value {
            continue;
        }
        // A run either starts above bit zero or wraps through it. In the
        // latter case the high run length is its right rotation from bit zero.
        let rotation = if element & 1 == 0 {
            size - element.trailing_zeros()
        } else {
            (element << (64 - size)).leading_ones()
        };
        let run = (1u64 << ones) - 1;
        let rotated = if rotation == 0 {
            run
        } else {
            ((run >> rotation) | (run << (size - rotation))) & mask
        };
        if rotated == element {
            return Some(LogicalImmediate {
                value,
                n: u32::from(size == 64),
                immr: rotation,
                imms: ((-(2 * size as i32)) as u32 | (ones - 1)) & 63,
            });
        }
    }
    None
}

// Keep the existing emitter's failure contract for invalid internal callers.
fn bitmask(value: u64, bits: u32) -> (u32, u32, u32) {
    let plan = logical_immediate(value, bits).expect("invalid codegen logical immediate");
    (plan.n, plan.immr, plan.imms)
}

pub(super) fn lower(op: &str, a: &[Arg]) -> Instruction {
    if op == "dmb" {
        let Arg::Barrier(n) = a[0] else {
            panic!("barrier")
        };
        return Instruction::new(O::DMB_BO_barriers, [(F::CRm, n)]);
    }
    if op == "brk" {
        return Instruction::new(O::BRK_EX_exception, [(F::imm16, a[0].number() as u32)]);
    }
    let d = a[0].reg();
    let t = tag(d);
    let bits = d.bits();
    match op {
        "cmp" | "cmn" => {
            let mut args = [zero(d); 4];
            args[1..a.len() + 1].copy_from_slice(a);
            return lower(
                if op == "cmp" { "subs" } else { "adds" },
                &args[..a.len() + 1],
            );
        }
        "tst" => return lower("ands", &[zero(d), a[0], a[1]]),
        "neg" | "negs" | "mvn" | "ngc" => {
            return lower(
                match op {
                    "neg" => "sub",
                    "negs" => "subs",
                    "mvn" => "orn",
                    _ => "sbc",
                },
                &[a[0], zero(d), a[1]],
            );
        }
        "mov" => {
            if let Arg::Imm(value) = a[1] {
                let mask = u64::MAX >> (64 - bits);
                let value = value as u64 & mask;
                for (invert, pattern) in [(false, value), (true, !value & mask)] {
                    for lane in (0..bits).step_by(16) {
                        if pattern & !(0xffffu64 << lane) == 0 {
                            return Instruction::new(
                                match (bits, invert) {
                                    (64, false) => O::MOVZ_64_movewide,
                                    (64, true) => O::MOVN_64_movewide,
                                    (_, false) => O::MOVZ_32_movewide,
                                    (_, true) => O::MOVN_32_movewide,
                                },
                                [
                                    (F::Rd, d.number),
                                    (F::hw, lane / 16),
                                    (F::imm16, (pattern >> lane) as u32),
                                ],
                            );
                        }
                    }
                }
                return lower("orr", &[a[0], zero(d), a[1]]);
            }
            let n = a[1].reg();
            return if d.class == 'p' || n.class == 'p' {
                lower("add", &[a[0], a[1], Arg::imm(0)])
            } else {
                lower("orr", &[a[0], zero(d), a[1]])
            };
        }
        _ => {}
    }
    if matches!(
        op,
        "ldr" | "str" | "ldrb" | "strb" | "ldrh" | "strh" | "ldrsw" | "ldp" | "stp"
    ) {
        return memory(op, a);
    }
    let n = a
        .get(1)
        .and_then(|a| if let Arg::Reg(r) = a { Some(*r) } else { None });
    let rn = n.map_or(0, |r| r.number);
    let m = a
        .get(2)
        .and_then(|a| if let Arg::Reg(r) = a { Some(*r) } else { None });
    let rm = m.map_or(0, |r| r.number);
    let regs = [(F::Rd, d.number), (F::Rn, rn), (F::Rm, rm)];
    match op {
        "add" | "sub" | "adds" | "subs" => {
            let flags = op.ends_with('s');
            let (kind, amount) = modifier(a.get(3));
            if let Arg::Imm(imm) = a[2] {
                return ins(
                    op,
                    if flags { flagtag(d) } else { t },
                    "addsub_imm",
                    [
                        (F::Rd, d.number),
                        (F::Rn, rn),
                        (F::imm12, imm as u32),
                        (F::sh, amount / 12),
                    ],
                );
            }
            let extended =
                d.class == 'p' || n.unwrap().class == 'p' || kind.starts_with(['u', 's']);
            if extended {
                let option = if kind == "lsl" {
                    if m.unwrap().bits() == 64 { 3 } else { 2 }
                } else {
                    extension(kind)
                };
                return ins(
                    op,
                    if flags { flagtag(d) } else { t },
                    "addsub_ext",
                    [
                        (F::Rd, d.number),
                        (F::Rn, rn),
                        (F::Rm, rm),
                        (F::option, option),
                        (F::imm3, amount),
                    ],
                );
            }
            ins(
                op,
                t,
                "addsub_shift",
                regs.into_iter().chain([
                    (
                        F::shift,
                        match kind {
                            "lsl" => 0,
                            "lsr" => 1,
                            "asr" => 2,
                            _ => unreachable!(),
                        },
                    ),
                    (F::imm6, amount),
                ]),
            )
        }
        "and" | "ands" | "orr" | "eor" | "orn" => {
            if let Arg::Imm(value) = a[2] {
                let value = (value as u64) & (u64::MAX >> (64 - bits));
                let (bit, immr, imms) = bitmask(value, bits);
                let mut i = ins(
                    op,
                    if op == "ands" { flagtag(d) } else { t },
                    "log_imm",
                    [
                        (F::Rd, d.number),
                        (F::Rn, rn),
                        (F::immr, immr),
                        (F::imms, imms),
                    ],
                );
                if bits == 64 {
                    i.fields.push((F::N, bit));
                }
                return i;
            }
            let (kind, amount) = modifier(a.get(3));
            ins(
                op,
                t,
                "log_shift",
                regs.into_iter().chain([
                    (
                        F::shift,
                        match kind {
                            "lsl" => 0,
                            "lsr" => 1,
                            "asr" => 2,
                            "ror" => 3,
                            _ => unreachable!(),
                        },
                    ),
                    (F::imm6, amount),
                ]),
            )
        }
        "adc" | "adcs" | "sbc" | "sbcs" => ins(op, t, "addsub_carry", regs),
        "sdiv" | "udiv" => ins(op, t, "dp_2src", regs),
        "clz" | "rbit" | "rev" | "rev16" => ins(op, t, "dp_1src", regs[..2].iter().copied()),
        "mul" | "madd" | "msub" => ins(
            if op == "msub" { "msub" } else { "madd" },
            if bits == 64 { "64A" } else { "32A" },
            "dp_3src",
            regs.into_iter()
                .chain([(F::Ra, if op == "mul" { 31 } else { a[3].reg().number })]),
        ),
        "smulh" | "umulh" => ins(op, "64", "dp_3src", regs),
        "smull" | "umull" => ins(
            if op == "smull" { "smaddl" } else { "umaddl" },
            "64WA",
            "dp_3src",
            regs.into_iter().chain([(F::Ra, 31)]),
        ),
        "csel" | "cset" => ins(
            if op == "cset" { "csinc" } else { op },
            t,
            "condsel",
            [
                (F::Rd, d.number),
                (F::Rn, if op == "cset" { 31 } else { rn }),
                (F::Rm, if op == "cset" { 31 } else { rm }),
                (F::cond, cc(*a.last().unwrap()) ^ u32::from(op == "cset")),
            ],
        ),
        "ccmp" => ins(
            op,
            t,
            "condcmp_reg",
            [
                (F::Rn, d.number),
                (F::Rm, rn),
                (F::nzcv, a[2].number() as u32),
                (F::cond, cc(a[3])),
            ],
        ),
        "lsl" | "lsr" | "asr" | "ror" if matches!(a[2], Arg::Reg(_)) => ins(
            match op {
                "lsl" => "lslv",
                "lsr" => "lsrv",
                "asr" => "asrv",
                _ => "rorv",
            },
            t,
            "dp_2src",
            regs,
        ),
        "lsl" | "lsr" | "asr" | "ror" | "sxtb" | "sxth" | "sxtw" | "uxtb" | "uxth" | "bfxil" => {
            let first = a.get(2).map_or(0, |a| a.number() as u32);
            if op == "ror" {
                return ins(
                    "extr",
                    t,
                    "extract",
                    regs[..2]
                        .iter()
                        .copied()
                        .chain([(F::Rm, rn), (F::imms, first)]),
                );
            }
            let (canonical, immr, imms) = match op {
                "lsl" => ("ubfm", (bits - first) % bits, bits - 1 - first),
                "lsr" => ("ubfm", first, bits - 1),
                "asr" => ("sbfm", first, bits - 1),
                "bfxil" => ("bfm", first, first + a[3].number() as u32 - 1),
                _ => (
                    if op.starts_with('s') { "sbfm" } else { "ubfm" },
                    0,
                    match op {
                        "sxtb" | "uxtb" => 7,
                        "sxth" | "uxth" => 15,
                        _ => 31,
                    },
                ),
            };
            ins(
                canonical,
                if bits == 64 { "64M" } else { "32M" },
                "bitfield",
                [
                    (F::Rd, d.number),
                    (F::Rn, rn),
                    (F::immr, immr),
                    (F::imms, imms),
                ],
            )
        }
        "fmov" => {
            let n = n.unwrap();
            let (tag, form) = if d.fp() && n.fp() {
                (t, "floatdp1")
            } else {
                (
                    match (d.class, n.class) {
                        ('s', _) => "S32",
                        ('d', _) => "D64",
                        (_, 's') => "32S",
                        (_, 'd') => "64D",
                        _ => unreachable!(),
                    },
                    "float2int",
                )
            };
            ins(op, tag, form, regs[..2].iter().copied())
        }
        "fcvt" => ins(
            op,
            if d.class == 's' { "SD" } else { "DS" },
            "floatdp1",
            regs[..2].iter().copied(),
        ),
        "scvtf" | "ucvtf" | "fcvtzs" | "fcvtzu" => ins(
            op,
            match (d.class, n.unwrap().class) {
                ('s', 'w') => "S32",
                ('s', _) => "S64",
                ('d', 'w') => "D32",
                ('d', _) => "D64",
                ('w', 's') => "32S",
                ('w', _) => "32D",
                (_, 's') => "64S",
                _ => "64D",
            },
            "float2int",
            regs[..2].iter().copied(),
        ),
        "fabs" | "fneg" | "fsqrt" | "frintn" | "frintp" | "frintm" | "frintz" => {
            ins(op, t, "floatdp1", regs[..2].iter().copied())
        }
        "fadd" | "fsub" | "fmul" | "fdiv" | "fmin" | "fmax" => ins(op, t, "floatdp2", regs),
        "fmadd" => ins(
            op,
            t,
            "floatdp3",
            regs.into_iter().chain([(F::Ra, a[3].reg().number)]),
        ),
        "fcmp" => ins(op, t, "floatcmp", [(F::Rn, d.number), (F::Rm, rn)]),
        "fcsel" => ins(
            op,
            t,
            "floatsel",
            regs.into_iter().chain([(F::cond, cc(a[3]))]),
        ),
        "cnt" => Instruction::new(
            O::CNT_asimdmisc_R,
            [(F::Rd, d.number), (F::Rn, rn), (F::Q, 0), (F::size, 0)],
        ),
        "addv" => Instruction::new(
            O::ADDV_asimdall_only,
            [(F::Rd, d.number), (F::Rn, rn), (F::Q, 0), (F::size, 0)],
        ),
        "br" | "blr" => ins(op, "64", "branch_reg", [(F::Rn, d.number)]),
        _ => panic!("missing native lowering: {op} {a:?}"),
    }
}

fn memory(op: &str, a: &[Arg]) -> Instruction {
    let r = a[0].reg();
    let pair = op == "ldp" || op == "stp";
    let at = if pair { 2 } else { 1 };
    let Arg::Memory(address) = a[at] else {
        panic!("expected address")
    };
    let offset = match address.offset {
        Offset::Immediate(n) => n.unwrap_or(0),
        _ => 0,
    };
    let post = a.get(at + 1).map(|a| a.number());
    let offset = post.unwrap_or(offset);
    let mode = if post.is_some() {
        "post"
    } else if address.pre {
        "pre"
    } else {
        "off"
    };
    if pair {
        let scale = r.bits() / 8;
        assert_eq!(offset % i128::from(scale), 0);
        let scaled = offset / i128::from(scale);
        assert!(
            (-64..64).contains(&scaled),
            "codegen pair offset out of range"
        );
        let imm = scaled as u32 & 127;
        return ins(
            op,
            tag(r),
            match mode {
                "pre" => "ldstpair_pre",
                "post" => "ldstpair_post",
                _ => "ldstpair_off",
            },
            [
                (F::Rt, r.number),
                (F::Rt2, a[1].reg().number),
                (F::Rn, address.base.number),
                (F::imm7, imm),
            ],
        );
    }
    let bytes = match op {
        "ldrb" | "strb" => 1,
        "ldrh" | "strh" => 2,
        "ldrsw" => 4,
        _ => r.bits() / 8,
    };
    if let Offset::Index { reg, extend, shift } = address.offset {
        // Byte LSL is a distinct encoding identity with option=3 fixed in
        // its opcode, rather than a writable `option` field.
        let byte = matches!(op, "ldrb" | "strb");
        let option = extension(extend);
        let byte_lsl = byte && option == 3;
        return ins(
            op,
            if byte_lsl {
                "32BL"
            } else if byte {
                "32B"
            } else {
                tag(r)
            },
            "ldst_regoff",
            [
                (F::Rt, r.number),
                (F::Rn, address.base.number),
                (F::Rm, reg.number),
                (F::option, option),
                (
                    F::S,
                    u32::from(shift.is_some() && shift.unwrap() == bytes.trailing_zeros()),
                ),
            ]
            .into_iter()
            .filter(|&(field, _)| !byte_lsl || field != F::option),
        );
    }
    if mode == "off"
        && offset >= 0
        && offset % i128::from(bytes) == 0
        && offset / i128::from(bytes) <= 4095
    {
        return ins(
            op,
            tag(r),
            "ldst_pos",
            [
                (F::Rt, r.number),
                (F::Rn, address.base.number),
                (F::imm12, (offset / i128::from(bytes)) as u32),
            ],
        );
    }
    let (op, form) = match mode {
        "pre" => (op, "ldst_immpre"),
        "post" => (op, "ldst_immpost"),
        _ => (
            match op {
                "ldr" => "ldur",
                "str" => "stur",
                "ldrb" => "ldurb",
                "strb" => "sturb",
                "ldrh" => "ldurh",
                "strh" => "sturh",
                "ldrsw" => "ldursw",
                _ => unreachable!(),
            },
            "ldst_unscaled",
        ),
    };
    assert!(
        (-256..256).contains(&offset),
        "codegen memory offset out of range"
    );
    ins(
        op,
        tag(r),
        form,
        [
            (F::Rt, r.number),
            (F::Rn, address.base.number),
            (F::imm9, offset as u32 & 511),
        ],
    )
}

// Static encoding selection: no mnemonic lookup or operand parsing.
fn opcode(op: &str, tag: &str, form: &str) -> O {
    match (op, tag, form) {
        ("adcs", "32", "addsub_carry") => O::ADCS_32_addsub_carry,
        ("adcs", "64", "addsub_carry") => O::ADCS_64_addsub_carry,
        ("adc", "32", "addsub_carry") => O::ADC_32_addsub_carry,
        ("adc", "64", "addsub_carry") => O::ADC_64_addsub_carry,
        ("adds", "32S", "addsub_ext") => O::ADDS_32S_addsub_ext,
        ("adds", "32S", "addsub_imm") => O::ADDS_32S_addsub_imm,
        ("adds", "32", "addsub_shift") => O::ADDS_32_addsub_shift,
        ("adds", "64S", "addsub_ext") => O::ADDS_64S_addsub_ext,
        ("adds", "64S", "addsub_imm") => O::ADDS_64S_addsub_imm,
        ("adds", "64", "addsub_shift") => O::ADDS_64_addsub_shift,
        ("add", "32", "addsub_ext") => O::ADD_32_addsub_ext,
        ("add", "32", "addsub_imm") => O::ADD_32_addsub_imm,
        ("add", "32", "addsub_shift") => O::ADD_32_addsub_shift,
        ("add", "64", "addsub_ext") => O::ADD_64_addsub_ext,
        ("add", "64", "addsub_imm") => O::ADD_64_addsub_imm,
        ("add", "64", "addsub_shift") => O::ADD_64_addsub_shift,
        ("add", "asimdsame", "only") => O::ADD_asimdsame_only,
        ("add", "asisdsame", "only") => O::ADD_asisdsame_only,
        ("ands", "32S", "log_imm") => O::ANDS_32S_log_imm,
        ("ands", "32", "log_shift") => O::ANDS_32_log_shift,
        ("ands", "64S", "log_imm") => O::ANDS_64S_log_imm,
        ("ands", "64", "log_shift") => O::ANDS_64_log_shift,
        ("and", "32", "log_imm") => O::AND_32_log_imm,
        ("and", "32", "log_shift") => O::AND_32_log_shift,
        ("and", "64", "log_imm") => O::AND_64_log_imm,
        ("and", "64", "log_shift") => O::AND_64_log_shift,
        ("and", "asimdsame", "only") => O::AND_asimdsame_only,
        ("asrv", "32", "dp_2src") => O::ASRV_32_dp_2src,
        ("asrv", "64", "dp_2src") => O::ASRV_64_dp_2src,
        ("bfm", "32M", "bitfield") => O::BFM_32M_bitfield,
        ("bfm", "64M", "bitfield") => O::BFM_64M_bitfield,
        ("blr", "64", "branch_reg") => O::BLR_64_branch_reg,
        ("br", "64", "branch_reg") => O::BR_64_branch_reg,
        ("ccmp", "32", "condcmp_imm") => O::CCMP_32_condcmp_imm,
        ("ccmp", "32", "condcmp_reg") => O::CCMP_32_condcmp_reg,
        ("ccmp", "64", "condcmp_imm") => O::CCMP_64_condcmp_imm,
        ("ccmp", "64", "condcmp_reg") => O::CCMP_64_condcmp_reg,
        ("clz", "32", "dp_1src") => O::CLZ_32_dp_1src,
        ("clz", "64", "dp_1src") => O::CLZ_64_dp_1src,
        ("clz", "asimdmisc", "R") => O::CLZ_asimdmisc_R,
        ("csel", "32", "condsel") => O::CSEL_32_condsel,
        ("csel", "64", "condsel") => O::CSEL_64_condsel,
        ("csinc", "32", "condsel") => O::CSINC_32_condsel,
        ("csinc", "64", "condsel") => O::CSINC_64_condsel,
        ("eor", "32", "log_imm") => O::EOR_32_log_imm,
        ("eor", "32", "log_shift") => O::EOR_32_log_shift,
        ("eor", "64", "log_imm") => O::EOR_64_log_imm,
        ("eor", "64", "log_shift") => O::EOR_64_log_shift,
        ("eor", "asimdsame", "only") => O::EOR_asimdsame_only,
        ("extr", "32", "extract") => O::EXTR_32_extract,
        ("extr", "64", "extract") => O::EXTR_64_extract,
        ("fabs", "D", "floatdp1") => O::FABS_D_floatdp1,
        ("fabs", "H", "floatdp1") => O::FABS_H_floatdp1,
        ("fabs", "S", "floatdp1") => O::FABS_S_floatdp1,
        ("fabs", "asimdmisc", "R") => O::FABS_asimdmisc_R,
        ("fabs", "asimdmiscfp16", "R") => O::FABS_asimdmiscfp16_R,
        ("fadd", "D", "floatdp2") => O::FADD_D_floatdp2,
        ("fadd", "H", "floatdp2") => O::FADD_H_floatdp2,
        ("fadd", "S", "floatdp2") => O::FADD_S_floatdp2,
        ("fadd", "asimdsame", "only") => O::FADD_asimdsame_only,
        ("fadd", "asimdsamefp16", "only") => O::FADD_asimdsamefp16_only,
        ("fcmp", "DZ", "floatcmp") => O::FCMP_DZ_floatcmp,
        ("fcmp", "D", "floatcmp") => O::FCMP_D_floatcmp,
        ("fcmp", "HZ", "floatcmp") => O::FCMP_HZ_floatcmp,
        ("fcmp", "H", "floatcmp") => O::FCMP_H_floatcmp,
        ("fcmp", "SZ", "floatcmp") => O::FCMP_SZ_floatcmp,
        ("fcmp", "S", "floatcmp") => O::FCMP_S_floatcmp,
        ("fcsel", "D", "floatsel") => O::FCSEL_D_floatsel,
        ("fcsel", "H", "floatsel") => O::FCSEL_H_floatsel,
        ("fcsel", "S", "floatsel") => O::FCSEL_S_floatsel,
        ("fcvtzs", "32D", "float2fix") => O::FCVTZS_32D_float2fix,
        ("fcvtzs", "32D", "float2int") => O::FCVTZS_32D_float2int,
        ("fcvtzs", "32H", "float2fix") => O::FCVTZS_32H_float2fix,
        ("fcvtzs", "32H", "float2int") => O::FCVTZS_32H_float2int,
        ("fcvtzs", "32S", "float2fix") => O::FCVTZS_32S_float2fix,
        ("fcvtzs", "32S", "float2int") => O::FCVTZS_32S_float2int,
        ("fcvtzs", "64D", "float2fix") => O::FCVTZS_64D_float2fix,
        ("fcvtzs", "64D", "float2int") => O::FCVTZS_64D_float2int,
        ("fcvtzs", "64H", "float2fix") => O::FCVTZS_64H_float2fix,
        ("fcvtzs", "64H", "float2int") => O::FCVTZS_64H_float2int,
        ("fcvtzs", "64S", "float2fix") => O::FCVTZS_64S_float2fix,
        ("fcvtzs", "64S", "float2int") => O::FCVTZS_64S_float2int,
        ("fcvtzs", "asimdmisc", "R") => O::FCVTZS_asimdmisc_R,
        ("fcvtzs", "asimdmiscfp16", "R") => O::FCVTZS_asimdmiscfp16_R,
        ("fcvtzs", "asimdshf", "C") => O::FCVTZS_asimdshf_C,
        ("fcvtzs", "asisdmisc", "R") => O::FCVTZS_asisdmisc_R,
        ("fcvtzs", "asisdmiscfp16", "R") => O::FCVTZS_asisdmiscfp16_R,
        ("fcvtzs", "asisdshf", "C") => O::FCVTZS_asisdshf_C,
        ("fcvtzu", "32D", "float2fix") => O::FCVTZU_32D_float2fix,
        ("fcvtzu", "32D", "float2int") => O::FCVTZU_32D_float2int,
        ("fcvtzu", "32H", "float2fix") => O::FCVTZU_32H_float2fix,
        ("fcvtzu", "32H", "float2int") => O::FCVTZU_32H_float2int,
        ("fcvtzu", "32S", "float2fix") => O::FCVTZU_32S_float2fix,
        ("fcvtzu", "32S", "float2int") => O::FCVTZU_32S_float2int,
        ("fcvtzu", "64D", "float2fix") => O::FCVTZU_64D_float2fix,
        ("fcvtzu", "64D", "float2int") => O::FCVTZU_64D_float2int,
        ("fcvtzu", "64H", "float2fix") => O::FCVTZU_64H_float2fix,
        ("fcvtzu", "64H", "float2int") => O::FCVTZU_64H_float2int,
        ("fcvtzu", "64S", "float2fix") => O::FCVTZU_64S_float2fix,
        ("fcvtzu", "64S", "float2int") => O::FCVTZU_64S_float2int,
        ("fcvtzu", "asimdmisc", "R") => O::FCVTZU_asimdmisc_R,
        ("fcvtzu", "asimdmiscfp16", "R") => O::FCVTZU_asimdmiscfp16_R,
        ("fcvtzu", "asimdshf", "C") => O::FCVTZU_asimdshf_C,
        ("fcvtzu", "asisdmisc", "R") => O::FCVTZU_asisdmisc_R,
        ("fcvtzu", "asisdmiscfp16", "R") => O::FCVTZU_asisdmiscfp16_R,
        ("fcvtzu", "asisdshf", "C") => O::FCVTZU_asisdshf_C,
        ("fcvt", "DH", "floatdp1") => O::FCVT_DH_floatdp1,
        ("fcvt", "DS", "floatdp1") => O::FCVT_DS_floatdp1,
        ("fcvt", "HD", "floatdp1") => O::FCVT_HD_floatdp1,
        ("fcvt", "HS", "floatdp1") => O::FCVT_HS_floatdp1,
        ("fcvt", "SD", "floatdp1") => O::FCVT_SD_floatdp1,
        ("fcvt", "SH", "floatdp1") => O::FCVT_SH_floatdp1,
        ("fdiv", "D", "floatdp2") => O::FDIV_D_floatdp2,
        ("fdiv", "H", "floatdp2") => O::FDIV_H_floatdp2,
        ("fdiv", "S", "floatdp2") => O::FDIV_S_floatdp2,
        ("fdiv", "asimdsame", "only") => O::FDIV_asimdsame_only,
        ("fdiv", "asimdsamefp16", "only") => O::FDIV_asimdsamefp16_only,
        ("fmadd", "D", "floatdp3") => O::FMADD_D_floatdp3,
        ("fmadd", "H", "floatdp3") => O::FMADD_H_floatdp3,
        ("fmadd", "S", "floatdp3") => O::FMADD_S_floatdp3,
        ("fmax", "D", "floatdp2") => O::FMAX_D_floatdp2,
        ("fmax", "H", "floatdp2") => O::FMAX_H_floatdp2,
        ("fmax", "S", "floatdp2") => O::FMAX_S_floatdp2,
        ("fmax", "asimdsame", "only") => O::FMAX_asimdsame_only,
        ("fmax", "asimdsamefp16", "only") => O::FMAX_asimdsamefp16_only,
        ("fmin", "D", "floatdp2") => O::FMIN_D_floatdp2,
        ("fmin", "H", "floatdp2") => O::FMIN_H_floatdp2,
        ("fmin", "S", "floatdp2") => O::FMIN_S_floatdp2,
        ("fmin", "asimdsame", "only") => O::FMIN_asimdsame_only,
        ("fmin", "asimdsamefp16", "only") => O::FMIN_asimdsamefp16_only,
        ("fmov", "32H", "float2int") => O::FMOV_32H_float2int,
        ("fmov", "32S", "float2int") => O::FMOV_32S_float2int,
        ("fmov", "64D", "float2int") => O::FMOV_64D_float2int,
        ("fmov", "64H", "float2int") => O::FMOV_64H_float2int,
        ("fmov", "D64", "float2int") => O::FMOV_D64_float2int,
        ("fmov", "D", "floatdp1") => O::FMOV_D_floatdp1,
        ("fmov", "D", "floatimm") => O::FMOV_D_floatimm,
        ("fmov", "H32", "float2int") => O::FMOV_H32_float2int,
        ("fmov", "H64", "float2int") => O::FMOV_H64_float2int,
        ("fmov", "H", "floatdp1") => O::FMOV_H_floatdp1,
        ("fmov", "H", "floatimm") => O::FMOV_H_floatimm,
        ("fmov", "S32", "float2int") => O::FMOV_S32_float2int,
        ("fmov", "S", "floatdp1") => O::FMOV_S_floatdp1,
        ("fmov", "S", "floatimm") => O::FMOV_S_floatimm,
        ("fmov", "asimdimm", "D2_d") => O::FMOV_asimdimm_D2_d,
        ("fmov", "asimdimm", "H_h") => O::FMOV_asimdimm_H_h,
        ("fmov", "asimdimm", "S_s") => O::FMOV_asimdimm_S_s,
        ("fmul", "D", "floatdp2") => O::FMUL_D_floatdp2,
        ("fmul", "H", "floatdp2") => O::FMUL_H_floatdp2,
        ("fmul", "S", "floatdp2") => O::FMUL_S_floatdp2,
        ("fmul", "asimdelem", "RH_H") => O::FMUL_asimdelem_RH_H,
        ("fmul", "asimdelem", "R_SD") => O::FMUL_asimdelem_R_SD,
        ("fmul", "asimdsame", "only") => O::FMUL_asimdsame_only,
        ("fmul", "asimdsamefp16", "only") => O::FMUL_asimdsamefp16_only,
        ("fmul", "asisdelem", "RH_H") => O::FMUL_asisdelem_RH_H,
        ("fmul", "asisdelem", "R_SD") => O::FMUL_asisdelem_R_SD,
        ("fneg", "D", "floatdp1") => O::FNEG_D_floatdp1,
        ("fneg", "H", "floatdp1") => O::FNEG_H_floatdp1,
        ("fneg", "S", "floatdp1") => O::FNEG_S_floatdp1,
        ("fneg", "asimdmisc", "R") => O::FNEG_asimdmisc_R,
        ("fneg", "asimdmiscfp16", "R") => O::FNEG_asimdmiscfp16_R,
        ("frintm", "D", "floatdp1") => O::FRINTM_D_floatdp1,
        ("frintm", "H", "floatdp1") => O::FRINTM_H_floatdp1,
        ("frintm", "S", "floatdp1") => O::FRINTM_S_floatdp1,
        ("frintm", "asimdmisc", "R") => O::FRINTM_asimdmisc_R,
        ("frintm", "asimdmiscfp16", "R") => O::FRINTM_asimdmiscfp16_R,
        ("frintn", "D", "floatdp1") => O::FRINTN_D_floatdp1,
        ("frintn", "H", "floatdp1") => O::FRINTN_H_floatdp1,
        ("frintn", "S", "floatdp1") => O::FRINTN_S_floatdp1,
        ("frintn", "asimdmisc", "R") => O::FRINTN_asimdmisc_R,
        ("frintn", "asimdmiscfp16", "R") => O::FRINTN_asimdmiscfp16_R,
        ("frintp", "D", "floatdp1") => O::FRINTP_D_floatdp1,
        ("frintp", "H", "floatdp1") => O::FRINTP_H_floatdp1,
        ("frintp", "S", "floatdp1") => O::FRINTP_S_floatdp1,
        ("frintp", "asimdmisc", "R") => O::FRINTP_asimdmisc_R,
        ("frintp", "asimdmiscfp16", "R") => O::FRINTP_asimdmiscfp16_R,
        ("frintz", "D", "floatdp1") => O::FRINTZ_D_floatdp1,
        ("frintz", "H", "floatdp1") => O::FRINTZ_H_floatdp1,
        ("frintz", "S", "floatdp1") => O::FRINTZ_S_floatdp1,
        ("frintz", "asimdmisc", "R") => O::FRINTZ_asimdmisc_R,
        ("frintz", "asimdmiscfp16", "R") => O::FRINTZ_asimdmiscfp16_R,
        ("fsqrt", "D", "floatdp1") => O::FSQRT_D_floatdp1,
        ("fsqrt", "H", "floatdp1") => O::FSQRT_H_floatdp1,
        ("fsqrt", "S", "floatdp1") => O::FSQRT_S_floatdp1,
        ("fsqrt", "asimdmisc", "R") => O::FSQRT_asimdmisc_R,
        ("fsqrt", "asimdmiscfp16", "R") => O::FSQRT_asimdmiscfp16_R,
        ("fsub", "D", "floatdp2") => O::FSUB_D_floatdp2,
        ("fsub", "H", "floatdp2") => O::FSUB_H_floatdp2,
        ("fsub", "S", "floatdp2") => O::FSUB_S_floatdp2,
        ("fsub", "asimdsame", "only") => O::FSUB_asimdsame_only,
        ("fsub", "asimdsamefp16", "only") => O::FSUB_asimdsamefp16_only,
        ("ldp", "32", "ldstpair_off") => O::LDP_32_ldstpair_off,
        ("ldp", "32", "ldstpair_post") => O::LDP_32_ldstpair_post,
        ("ldp", "32", "ldstpair_pre") => O::LDP_32_ldstpair_pre,
        ("ldp", "64", "ldstpair_off") => O::LDP_64_ldstpair_off,
        ("ldp", "64", "ldstpair_post") => O::LDP_64_ldstpair_post,
        ("ldp", "64", "ldstpair_pre") => O::LDP_64_ldstpair_pre,
        ("ldp", "D", "ldstpair_off") => O::LDP_D_ldstpair_off,
        ("ldp", "D", "ldstpair_post") => O::LDP_D_ldstpair_post,
        ("ldp", "D", "ldstpair_pre") => O::LDP_D_ldstpair_pre,
        ("ldp", "Q", "ldstpair_off") => O::LDP_Q_ldstpair_off,
        ("ldp", "Q", "ldstpair_post") => O::LDP_Q_ldstpair_post,
        ("ldp", "Q", "ldstpair_pre") => O::LDP_Q_ldstpair_pre,
        ("ldp", "S", "ldstpair_off") => O::LDP_S_ldstpair_off,
        ("ldp", "S", "ldstpair_post") => O::LDP_S_ldstpair_post,
        ("ldp", "S", "ldstpair_pre") => O::LDP_S_ldstpair_pre,
        ("ldrb", "32BL", "ldst_regoff") => O::LDRB_32BL_ldst_regoff,
        ("ldrb", "32B", "ldst_regoff") => O::LDRB_32B_ldst_regoff,
        ("ldrb", "32", "ldst_immpost") => O::LDRB_32_ldst_immpost,
        ("ldrb", "32", "ldst_immpre") => O::LDRB_32_ldst_immpre,
        ("ldrb", "32", "ldst_pos") => O::LDRB_32_ldst_pos,
        ("ldrh", "32", "ldst_immpost") => O::LDRH_32_ldst_immpost,
        ("ldrh", "32", "ldst_immpre") => O::LDRH_32_ldst_immpre,
        ("ldrh", "32", "ldst_pos") => O::LDRH_32_ldst_pos,
        ("ldrh", "32", "ldst_regoff") => O::LDRH_32_ldst_regoff,
        ("ldrsw", "64", "ldst_immpost") => O::LDRSW_64_ldst_immpost,
        ("ldrsw", "64", "ldst_immpre") => O::LDRSW_64_ldst_immpre,
        ("ldrsw", "64", "ldst_pos") => O::LDRSW_64_ldst_pos,
        ("ldrsw", "64", "ldst_regoff") => O::LDRSW_64_ldst_regoff,
        ("ldrsw", "64", "loadlit") => O::LDRSW_64_loadlit,
        ("ldr", "32", "ldst_immpost") => O::LDR_32_ldst_immpost,
        ("ldr", "32", "ldst_immpre") => O::LDR_32_ldst_immpre,
        ("ldr", "32", "ldst_pos") => O::LDR_32_ldst_pos,
        ("ldr", "32", "ldst_regoff") => O::LDR_32_ldst_regoff,
        ("ldr", "32", "loadlit") => O::LDR_32_loadlit,
        ("ldr", "64", "ldst_immpost") => O::LDR_64_ldst_immpost,
        ("ldr", "64", "ldst_immpre") => O::LDR_64_ldst_immpre,
        ("ldr", "64", "ldst_pos") => O::LDR_64_ldst_pos,
        ("ldr", "64", "ldst_regoff") => O::LDR_64_ldst_regoff,
        ("ldr", "64", "loadlit") => O::LDR_64_loadlit,
        ("ldr", "BL", "ldst_regoff") => O::LDR_BL_ldst_regoff,
        ("ldr", "B", "ldst_immpost") => O::LDR_B_ldst_immpost,
        ("ldr", "B", "ldst_immpre") => O::LDR_B_ldst_immpre,
        ("ldr", "B", "ldst_pos") => O::LDR_B_ldst_pos,
        ("ldr", "B", "ldst_regoff") => O::LDR_B_ldst_regoff,
        ("ldr", "D", "ldst_immpost") => O::LDR_D_ldst_immpost,
        ("ldr", "D", "ldst_immpre") => O::LDR_D_ldst_immpre,
        ("ldr", "D", "ldst_pos") => O::LDR_D_ldst_pos,
        ("ldr", "D", "ldst_regoff") => O::LDR_D_ldst_regoff,
        ("ldr", "D", "loadlit") => O::LDR_D_loadlit,
        ("ldr", "H", "ldst_immpost") => O::LDR_H_ldst_immpost,
        ("ldr", "H", "ldst_immpre") => O::LDR_H_ldst_immpre,
        ("ldr", "H", "ldst_pos") => O::LDR_H_ldst_pos,
        ("ldr", "H", "ldst_regoff") => O::LDR_H_ldst_regoff,
        ("ldr", "Q", "ldst_immpost") => O::LDR_Q_ldst_immpost,
        ("ldr", "Q", "ldst_immpre") => O::LDR_Q_ldst_immpre,
        ("ldr", "Q", "ldst_pos") => O::LDR_Q_ldst_pos,
        ("ldr", "Q", "ldst_regoff") => O::LDR_Q_ldst_regoff,
        ("ldr", "Q", "loadlit") => O::LDR_Q_loadlit,
        ("ldr", "S", "ldst_immpost") => O::LDR_S_ldst_immpost,
        ("ldr", "S", "ldst_immpre") => O::LDR_S_ldst_immpre,
        ("ldr", "S", "ldst_pos") => O::LDR_S_ldst_pos,
        ("ldr", "S", "ldst_regoff") => O::LDR_S_ldst_regoff,
        ("ldr", "S", "loadlit") => O::LDR_S_loadlit,
        ("ldurb", "32", "ldst_unscaled") => O::LDURB_32_ldst_unscaled,
        ("ldurh", "32", "ldst_unscaled") => O::LDURH_32_ldst_unscaled,
        ("ldursw", "64", "ldst_unscaled") => O::LDURSW_64_ldst_unscaled,
        ("ldur", "32", "ldst_unscaled") => O::LDUR_32_ldst_unscaled,
        ("ldur", "64", "ldst_unscaled") => O::LDUR_64_ldst_unscaled,
        ("ldur", "B", "ldst_unscaled") => O::LDUR_B_ldst_unscaled,
        ("ldur", "D", "ldst_unscaled") => O::LDUR_D_ldst_unscaled,
        ("ldur", "H", "ldst_unscaled") => O::LDUR_H_ldst_unscaled,
        ("ldur", "Q", "ldst_unscaled") => O::LDUR_Q_ldst_unscaled,
        ("ldur", "S", "ldst_unscaled") => O::LDUR_S_ldst_unscaled,
        ("lslv", "32", "dp_2src") => O::LSLV_32_dp_2src,
        ("lslv", "64", "dp_2src") => O::LSLV_64_dp_2src,
        ("lsrv", "32", "dp_2src") => O::LSRV_32_dp_2src,
        ("lsrv", "64", "dp_2src") => O::LSRV_64_dp_2src,
        ("madd", "32A", "dp_3src") => O::MADD_32A_dp_3src,
        ("madd", "64A", "dp_3src") => O::MADD_64A_dp_3src,
        ("msub", "32A", "dp_3src") => O::MSUB_32A_dp_3src,
        ("msub", "64A", "dp_3src") => O::MSUB_64A_dp_3src,
        ("orn", "32", "log_shift") => O::ORN_32_log_shift,
        ("orn", "64", "log_shift") => O::ORN_64_log_shift,
        ("orn", "asimdsame", "only") => O::ORN_asimdsame_only,
        ("orr", "32", "log_imm") => O::ORR_32_log_imm,
        ("orr", "32", "log_shift") => O::ORR_32_log_shift,
        ("orr", "64", "log_imm") => O::ORR_64_log_imm,
        ("orr", "64", "log_shift") => O::ORR_64_log_shift,
        ("orr", "asimdimm", "L_hl") => O::ORR_asimdimm_L_hl,
        ("orr", "asimdimm", "L_sl") => O::ORR_asimdimm_L_sl,
        ("orr", "asimdsame", "only") => O::ORR_asimdsame_only,
        ("rbit", "32", "dp_1src") => O::RBIT_32_dp_1src,
        ("rbit", "64", "dp_1src") => O::RBIT_64_dp_1src,
        ("rbit", "asimdmisc", "R") => O::RBIT_asimdmisc_R,
        ("rev16", "32", "dp_1src") => O::REV16_32_dp_1src,
        ("rev16", "64", "dp_1src") => O::REV16_64_dp_1src,
        ("rev16", "asimdmisc", "R") => O::REV16_asimdmisc_R,
        ("rev", "32", "dp_1src") => O::REV_32_dp_1src,
        ("rev", "64", "dp_1src") => O::REV_64_dp_1src,
        ("rorv", "32", "dp_2src") => O::RORV_32_dp_2src,
        ("rorv", "64", "dp_2src") => O::RORV_64_dp_2src,
        ("sbcs", "32", "addsub_carry") => O::SBCS_32_addsub_carry,
        ("sbcs", "64", "addsub_carry") => O::SBCS_64_addsub_carry,
        ("sbc", "32", "addsub_carry") => O::SBC_32_addsub_carry,
        ("sbc", "64", "addsub_carry") => O::SBC_64_addsub_carry,
        ("sbfm", "32M", "bitfield") => O::SBFM_32M_bitfield,
        ("sbfm", "64M", "bitfield") => O::SBFM_64M_bitfield,
        ("scvtf", "D32", "float2fix") => O::SCVTF_D32_float2fix,
        ("scvtf", "D32", "float2int") => O::SCVTF_D32_float2int,
        ("scvtf", "D64", "float2fix") => O::SCVTF_D64_float2fix,
        ("scvtf", "D64", "float2int") => O::SCVTF_D64_float2int,
        ("scvtf", "H32", "float2fix") => O::SCVTF_H32_float2fix,
        ("scvtf", "H32", "float2int") => O::SCVTF_H32_float2int,
        ("scvtf", "H64", "float2fix") => O::SCVTF_H64_float2fix,
        ("scvtf", "H64", "float2int") => O::SCVTF_H64_float2int,
        ("scvtf", "S32", "float2fix") => O::SCVTF_S32_float2fix,
        ("scvtf", "S32", "float2int") => O::SCVTF_S32_float2int,
        ("scvtf", "S64", "float2fix") => O::SCVTF_S64_float2fix,
        ("scvtf", "S64", "float2int") => O::SCVTF_S64_float2int,
        ("scvtf", "asimdmisc", "R") => O::SCVTF_asimdmisc_R,
        ("scvtf", "asimdmiscfp16", "R") => O::SCVTF_asimdmiscfp16_R,
        ("scvtf", "asimdshf", "C") => O::SCVTF_asimdshf_C,
        ("scvtf", "asisdmisc", "R") => O::SCVTF_asisdmisc_R,
        ("scvtf", "asisdmiscfp16", "R") => O::SCVTF_asisdmiscfp16_R,
        ("scvtf", "asisdshf", "C") => O::SCVTF_asisdshf_C,
        ("sdiv", "32", "dp_2src") => O::SDIV_32_dp_2src,
        ("sdiv", "64", "dp_2src") => O::SDIV_64_dp_2src,
        ("smaddl", "64WA", "dp_3src") => O::SMADDL_64WA_dp_3src,
        ("smulh", "64", "dp_3src") => O::SMULH_64_dp_3src,
        ("stp", "32", "ldstpair_off") => O::STP_32_ldstpair_off,
        ("stp", "32", "ldstpair_post") => O::STP_32_ldstpair_post,
        ("stp", "32", "ldstpair_pre") => O::STP_32_ldstpair_pre,
        ("stp", "64", "ldstpair_off") => O::STP_64_ldstpair_off,
        ("stp", "64", "ldstpair_post") => O::STP_64_ldstpair_post,
        ("stp", "64", "ldstpair_pre") => O::STP_64_ldstpair_pre,
        ("stp", "D", "ldstpair_off") => O::STP_D_ldstpair_off,
        ("stp", "D", "ldstpair_post") => O::STP_D_ldstpair_post,
        ("stp", "D", "ldstpair_pre") => O::STP_D_ldstpair_pre,
        ("stp", "Q", "ldstpair_off") => O::STP_Q_ldstpair_off,
        ("stp", "Q", "ldstpair_post") => O::STP_Q_ldstpair_post,
        ("stp", "Q", "ldstpair_pre") => O::STP_Q_ldstpair_pre,
        ("stp", "S", "ldstpair_off") => O::STP_S_ldstpair_off,
        ("stp", "S", "ldstpair_post") => O::STP_S_ldstpair_post,
        ("stp", "S", "ldstpair_pre") => O::STP_S_ldstpair_pre,
        ("strb", "32BL", "ldst_regoff") => O::STRB_32BL_ldst_regoff,
        ("strb", "32B", "ldst_regoff") => O::STRB_32B_ldst_regoff,
        ("strb", "32", "ldst_immpost") => O::STRB_32_ldst_immpost,
        ("strb", "32", "ldst_immpre") => O::STRB_32_ldst_immpre,
        ("strb", "32", "ldst_pos") => O::STRB_32_ldst_pos,
        ("strh", "32", "ldst_immpost") => O::STRH_32_ldst_immpost,
        ("strh", "32", "ldst_immpre") => O::STRH_32_ldst_immpre,
        ("strh", "32", "ldst_pos") => O::STRH_32_ldst_pos,
        ("strh", "32", "ldst_regoff") => O::STRH_32_ldst_regoff,
        ("str", "32", "ldst_immpost") => O::STR_32_ldst_immpost,
        ("str", "32", "ldst_immpre") => O::STR_32_ldst_immpre,
        ("str", "32", "ldst_pos") => O::STR_32_ldst_pos,
        ("str", "32", "ldst_regoff") => O::STR_32_ldst_regoff,
        ("str", "64", "ldst_immpost") => O::STR_64_ldst_immpost,
        ("str", "64", "ldst_immpre") => O::STR_64_ldst_immpre,
        ("str", "64", "ldst_pos") => O::STR_64_ldst_pos,
        ("str", "64", "ldst_regoff") => O::STR_64_ldst_regoff,
        ("str", "BL", "ldst_regoff") => O::STR_BL_ldst_regoff,
        ("str", "B", "ldst_immpost") => O::STR_B_ldst_immpost,
        ("str", "B", "ldst_immpre") => O::STR_B_ldst_immpre,
        ("str", "B", "ldst_pos") => O::STR_B_ldst_pos,
        ("str", "B", "ldst_regoff") => O::STR_B_ldst_regoff,
        ("str", "D", "ldst_immpost") => O::STR_D_ldst_immpost,
        ("str", "D", "ldst_immpre") => O::STR_D_ldst_immpre,
        ("str", "D", "ldst_pos") => O::STR_D_ldst_pos,
        ("str", "D", "ldst_regoff") => O::STR_D_ldst_regoff,
        ("str", "H", "ldst_immpost") => O::STR_H_ldst_immpost,
        ("str", "H", "ldst_immpre") => O::STR_H_ldst_immpre,
        ("str", "H", "ldst_pos") => O::STR_H_ldst_pos,
        ("str", "H", "ldst_regoff") => O::STR_H_ldst_regoff,
        ("str", "Q", "ldst_immpost") => O::STR_Q_ldst_immpost,
        ("str", "Q", "ldst_immpre") => O::STR_Q_ldst_immpre,
        ("str", "Q", "ldst_pos") => O::STR_Q_ldst_pos,
        ("str", "Q", "ldst_regoff") => O::STR_Q_ldst_regoff,
        ("str", "S", "ldst_immpost") => O::STR_S_ldst_immpost,
        ("str", "S", "ldst_immpre") => O::STR_S_ldst_immpre,
        ("str", "S", "ldst_pos") => O::STR_S_ldst_pos,
        ("str", "S", "ldst_regoff") => O::STR_S_ldst_regoff,
        ("sturb", "32", "ldst_unscaled") => O::STURB_32_ldst_unscaled,
        ("sturh", "32", "ldst_unscaled") => O::STURH_32_ldst_unscaled,
        ("stur", "32", "ldst_unscaled") => O::STUR_32_ldst_unscaled,
        ("stur", "64", "ldst_unscaled") => O::STUR_64_ldst_unscaled,
        ("stur", "B", "ldst_unscaled") => O::STUR_B_ldst_unscaled,
        ("stur", "D", "ldst_unscaled") => O::STUR_D_ldst_unscaled,
        ("stur", "H", "ldst_unscaled") => O::STUR_H_ldst_unscaled,
        ("stur", "Q", "ldst_unscaled") => O::STUR_Q_ldst_unscaled,
        ("stur", "S", "ldst_unscaled") => O::STUR_S_ldst_unscaled,
        ("subs", "32S", "addsub_ext") => O::SUBS_32S_addsub_ext,
        ("subs", "32S", "addsub_imm") => O::SUBS_32S_addsub_imm,
        ("subs", "32", "addsub_shift") => O::SUBS_32_addsub_shift,
        ("subs", "64S", "addsub_ext") => O::SUBS_64S_addsub_ext,
        ("subs", "64S", "addsub_imm") => O::SUBS_64S_addsub_imm,
        ("subs", "64", "addsub_shift") => O::SUBS_64_addsub_shift,
        ("sub", "32", "addsub_ext") => O::SUB_32_addsub_ext,
        ("sub", "32", "addsub_imm") => O::SUB_32_addsub_imm,
        ("sub", "32", "addsub_shift") => O::SUB_32_addsub_shift,
        ("sub", "64", "addsub_ext") => O::SUB_64_addsub_ext,
        ("sub", "64", "addsub_imm") => O::SUB_64_addsub_imm,
        ("sub", "64", "addsub_shift") => O::SUB_64_addsub_shift,
        ("sub", "asimdsame", "only") => O::SUB_asimdsame_only,
        ("sub", "asisdsame", "only") => O::SUB_asisdsame_only,
        ("ubfm", "32M", "bitfield") => O::UBFM_32M_bitfield,
        ("ubfm", "64M", "bitfield") => O::UBFM_64M_bitfield,
        ("ucvtf", "D32", "float2fix") => O::UCVTF_D32_float2fix,
        ("ucvtf", "D32", "float2int") => O::UCVTF_D32_float2int,
        ("ucvtf", "D64", "float2fix") => O::UCVTF_D64_float2fix,
        ("ucvtf", "D64", "float2int") => O::UCVTF_D64_float2int,
        ("ucvtf", "H32", "float2fix") => O::UCVTF_H32_float2fix,
        ("ucvtf", "H32", "float2int") => O::UCVTF_H32_float2int,
        ("ucvtf", "H64", "float2fix") => O::UCVTF_H64_float2fix,
        ("ucvtf", "H64", "float2int") => O::UCVTF_H64_float2int,
        ("ucvtf", "S32", "float2fix") => O::UCVTF_S32_float2fix,
        ("ucvtf", "S32", "float2int") => O::UCVTF_S32_float2int,
        ("ucvtf", "S64", "float2fix") => O::UCVTF_S64_float2fix,
        ("ucvtf", "S64", "float2int") => O::UCVTF_S64_float2int,
        ("ucvtf", "asimdmisc", "R") => O::UCVTF_asimdmisc_R,
        ("ucvtf", "asimdmiscfp16", "R") => O::UCVTF_asimdmiscfp16_R,
        ("ucvtf", "asimdshf", "C") => O::UCVTF_asimdshf_C,
        ("ucvtf", "asisdmisc", "R") => O::UCVTF_asisdmisc_R,
        ("ucvtf", "asisdmiscfp16", "R") => O::UCVTF_asisdmiscfp16_R,
        ("ucvtf", "asisdshf", "C") => O::UCVTF_asisdshf_C,
        ("udiv", "32", "dp_2src") => O::UDIV_32_dp_2src,
        ("udiv", "64", "dp_2src") => O::UDIV_64_dp_2src,
        ("umaddl", "64WA", "dp_3src") => O::UMADDL_64WA_dp_3src,
        ("umulh", "64", "dp_3src") => O::UMULH_64_dp_3src,
        ("add", "z", "zz_") => O::add_z_zz_,
        ("add", "za", "zw_2x2") => O::add_za_zw_2x2,
        ("add", "za", "zw_4x4") => O::add_za_zw_4x4,
        ("add", "za", "zzv_2x1") => O::add_za_zzv_2x1,
        ("add", "za", "zzv_4x1") => O::add_za_zzv_4x1,
        ("add", "za", "zzw_2x2") => O::add_za_zzw_2x2,
        ("add", "za", "zzw_4x4") => O::add_za_zzw_4x4,
        ("and", "z", "zz_") => O::and_z_zz_,
        ("eor", "z", "zz_") => O::eor_z_zz_,
        ("fadd", "z", "zz_") => O::fadd_z_zz_,
        ("fmul", "z", "zz_") => O::fmul_z_zz_,
        ("fsub", "z", "zz_") => O::fsub_z_zz_,
        ("ldr", "za", "ri_") => O::ldr_za_ri_,
        ("ldr", "zt", "br_") => O::ldr_zt_br_,
        ("orr", "z", "zz_") => O::orr_z_zz_,
        ("str", "za", "ri_") => O::str_za_ri_,
        ("str", "zt", "br_") => O::str_zt_br_,
        ("sub", "z", "zz_") => O::sub_z_zz_,
        ("sub", "za", "zw_2x2") => O::sub_za_zw_2x2,
        ("sub", "za", "zw_4x4") => O::sub_za_zw_4x4,
        ("sub", "za", "zzv_2x1") => O::sub_za_zzv_2x1,
        ("sub", "za", "zzv_4x1") => O::sub_za_zzv_4x1,
        ("sub", "za", "zzw_2x2") => O::sub_za_zzw_2x2,
        ("sub", "za", "zzw_4x4") => O::sub_za_zzw_4x4,
        _ => panic!("missing codegen encoding: {op} {tag} {form}"),
    }
}
pub(super) fn atomic(root: &str, order: &str, width: &str, a: &[Arg]) -> Instruction {
    let r = a[0].reg();
    let Arg::Memory(address) = *a.last().unwrap() else {
        panic!("atomic memory")
    };
    if root == "ldr" || root == "str" {
        return memory(
            match (root, width) {
                ("ldr", "b") => "ldrb",
                ("ldr", "h") => "ldrh",
                ("str", "b") => "strb",
                ("str", "h") => "strh",
                _ => root,
            },
            a,
        );
    }
    let opcode = atomic_opcode(root, order, width, r.bits());
    if a.len() == 2 {
        Instruction::new(opcode, [(F::Rt, r.number), (F::Rn, address.base.number)])
    } else {
        Instruction::new(
            opcode,
            [
                (F::Rs, r.number),
                (F::Rt, a[1].reg().number),
                (F::Rn, address.base.number),
            ],
        )
    }
}
fn atomic_opcode(root: &str, order: &str, width: &str, bits: u32) -> O {
    match (root, order, width, bits) {
        ("ldadd", "", "", 32) => O::LDADD_32_memop,
        ("ldadd", "", "", 64) => O::LDADD_64_memop,
        ("ldadd", "", "b", 32) => O::LDADDB_32_memop,
        ("ldadd", "", "h", 32) => O::LDADDH_32_memop,
        ("ldadd", "a", "", 32) => O::LDADDA_32_memop,
        ("ldadd", "a", "", 64) => O::LDADDA_64_memop,
        ("ldadd", "a", "b", 32) => O::LDADDAB_32_memop,
        ("ldadd", "a", "h", 32) => O::LDADDAH_32_memop,
        ("ldadd", "l", "", 32) => O::LDADDL_32_memop,
        ("ldadd", "l", "", 64) => O::LDADDL_64_memop,
        ("ldadd", "l", "b", 32) => O::LDADDLB_32_memop,
        ("ldadd", "l", "h", 32) => O::LDADDLH_32_memop,
        ("ldadd", "al", "", 32) => O::LDADDAL_32_memop,
        ("ldadd", "al", "", 64) => O::LDADDAL_64_memop,
        ("ldadd", "al", "b", 32) => O::LDADDALB_32_memop,
        ("ldadd", "al", "h", 32) => O::LDADDALH_32_memop,
        ("ldclr", "", "", 32) => O::LDCLR_32_memop,
        ("ldclr", "", "", 64) => O::LDCLR_64_memop,
        ("ldclr", "", "b", 32) => O::LDCLRB_32_memop,
        ("ldclr", "", "h", 32) => O::LDCLRH_32_memop,
        ("ldclr", "a", "", 32) => O::LDCLRA_32_memop,
        ("ldclr", "a", "", 64) => O::LDCLRA_64_memop,
        ("ldclr", "a", "b", 32) => O::LDCLRAB_32_memop,
        ("ldclr", "a", "h", 32) => O::LDCLRAH_32_memop,
        ("ldclr", "l", "", 32) => O::LDCLRL_32_memop,
        ("ldclr", "l", "", 64) => O::LDCLRL_64_memop,
        ("ldclr", "l", "b", 32) => O::LDCLRLB_32_memop,
        ("ldclr", "l", "h", 32) => O::LDCLRLH_32_memop,
        ("ldclr", "al", "", 32) => O::LDCLRAL_32_memop,
        ("ldclr", "al", "", 64) => O::LDCLRAL_64_memop,
        ("ldclr", "al", "b", 32) => O::LDCLRALB_32_memop,
        ("ldclr", "al", "h", 32) => O::LDCLRALH_32_memop,
        ("ldset", "", "", 32) => O::LDSET_32_memop,
        ("ldset", "", "", 64) => O::LDSET_64_memop,
        ("ldset", "", "b", 32) => O::LDSETB_32_memop,
        ("ldset", "", "h", 32) => O::LDSETH_32_memop,
        ("ldset", "a", "", 32) => O::LDSETA_32_memop,
        ("ldset", "a", "", 64) => O::LDSETA_64_memop,
        ("ldset", "a", "b", 32) => O::LDSETAB_32_memop,
        ("ldset", "a", "h", 32) => O::LDSETAH_32_memop,
        ("ldset", "l", "", 32) => O::LDSETL_32_memop,
        ("ldset", "l", "", 64) => O::LDSETL_64_memop,
        ("ldset", "l", "b", 32) => O::LDSETLB_32_memop,
        ("ldset", "l", "h", 32) => O::LDSETLH_32_memop,
        ("ldset", "al", "", 32) => O::LDSETAL_32_memop,
        ("ldset", "al", "", 64) => O::LDSETAL_64_memop,
        ("ldset", "al", "b", 32) => O::LDSETALB_32_memop,
        ("ldset", "al", "h", 32) => O::LDSETALH_32_memop,
        ("ldeor", "", "", 32) => O::LDEOR_32_memop,
        ("ldeor", "", "", 64) => O::LDEOR_64_memop,
        ("ldeor", "", "b", 32) => O::LDEORB_32_memop,
        ("ldeor", "", "h", 32) => O::LDEORH_32_memop,
        ("ldeor", "a", "", 32) => O::LDEORA_32_memop,
        ("ldeor", "a", "", 64) => O::LDEORA_64_memop,
        ("ldeor", "a", "b", 32) => O::LDEORAB_32_memop,
        ("ldeor", "a", "h", 32) => O::LDEORAH_32_memop,
        ("ldeor", "l", "", 32) => O::LDEORL_32_memop,
        ("ldeor", "l", "", 64) => O::LDEORL_64_memop,
        ("ldeor", "l", "b", 32) => O::LDEORLB_32_memop,
        ("ldeor", "l", "h", 32) => O::LDEORLH_32_memop,
        ("ldeor", "al", "", 32) => O::LDEORAL_32_memop,
        ("ldeor", "al", "", 64) => O::LDEORAL_64_memop,
        ("ldeor", "al", "b", 32) => O::LDEORALB_32_memop,
        ("ldeor", "al", "h", 32) => O::LDEORALH_32_memop,
        ("ldsmin", "", "", 32) => O::LDSMIN_32_memop,
        ("ldsmin", "", "", 64) => O::LDSMIN_64_memop,
        ("ldsmin", "", "b", 32) => O::LDSMINB_32_memop,
        ("ldsmin", "", "h", 32) => O::LDSMINH_32_memop,
        ("ldsmin", "a", "", 32) => O::LDSMINA_32_memop,
        ("ldsmin", "a", "", 64) => O::LDSMINA_64_memop,
        ("ldsmin", "a", "b", 32) => O::LDSMINAB_32_memop,
        ("ldsmin", "a", "h", 32) => O::LDSMINAH_32_memop,
        ("ldsmin", "l", "", 32) => O::LDSMINL_32_memop,
        ("ldsmin", "l", "", 64) => O::LDSMINL_64_memop,
        ("ldsmin", "l", "b", 32) => O::LDSMINLB_32_memop,
        ("ldsmin", "l", "h", 32) => O::LDSMINLH_32_memop,
        ("ldsmin", "al", "", 32) => O::LDSMINAL_32_memop,
        ("ldsmin", "al", "", 64) => O::LDSMINAL_64_memop,
        ("ldsmin", "al", "b", 32) => O::LDSMINALB_32_memop,
        ("ldsmin", "al", "h", 32) => O::LDSMINALH_32_memop,
        ("ldsmax", "", "", 32) => O::LDSMAX_32_memop,
        ("ldsmax", "", "", 64) => O::LDSMAX_64_memop,
        ("ldsmax", "", "b", 32) => O::LDSMAXB_32_memop,
        ("ldsmax", "", "h", 32) => O::LDSMAXH_32_memop,
        ("ldsmax", "a", "", 32) => O::LDSMAXA_32_memop,
        ("ldsmax", "a", "", 64) => O::LDSMAXA_64_memop,
        ("ldsmax", "a", "b", 32) => O::LDSMAXAB_32_memop,
        ("ldsmax", "a", "h", 32) => O::LDSMAXAH_32_memop,
        ("ldsmax", "l", "", 32) => O::LDSMAXL_32_memop,
        ("ldsmax", "l", "", 64) => O::LDSMAXL_64_memop,
        ("ldsmax", "l", "b", 32) => O::LDSMAXLB_32_memop,
        ("ldsmax", "l", "h", 32) => O::LDSMAXLH_32_memop,
        ("ldsmax", "al", "", 32) => O::LDSMAXAL_32_memop,
        ("ldsmax", "al", "", 64) => O::LDSMAXAL_64_memop,
        ("ldsmax", "al", "b", 32) => O::LDSMAXALB_32_memop,
        ("ldsmax", "al", "h", 32) => O::LDSMAXALH_32_memop,
        ("ldumin", "", "", 32) => O::LDUMIN_32_memop,
        ("ldumin", "", "", 64) => O::LDUMIN_64_memop,
        ("ldumin", "", "b", 32) => O::LDUMINB_32_memop,
        ("ldumin", "", "h", 32) => O::LDUMINH_32_memop,
        ("ldumin", "a", "", 32) => O::LDUMINA_32_memop,
        ("ldumin", "a", "", 64) => O::LDUMINA_64_memop,
        ("ldumin", "a", "b", 32) => O::LDUMINAB_32_memop,
        ("ldumin", "a", "h", 32) => O::LDUMINAH_32_memop,
        ("ldumin", "l", "", 32) => O::LDUMINL_32_memop,
        ("ldumin", "l", "", 64) => O::LDUMINL_64_memop,
        ("ldumin", "l", "b", 32) => O::LDUMINLB_32_memop,
        ("ldumin", "l", "h", 32) => O::LDUMINLH_32_memop,
        ("ldumin", "al", "", 32) => O::LDUMINAL_32_memop,
        ("ldumin", "al", "", 64) => O::LDUMINAL_64_memop,
        ("ldumin", "al", "b", 32) => O::LDUMINALB_32_memop,
        ("ldumin", "al", "h", 32) => O::LDUMINALH_32_memop,
        ("ldumax", "", "", 32) => O::LDUMAX_32_memop,
        ("ldumax", "", "", 64) => O::LDUMAX_64_memop,
        ("ldumax", "", "b", 32) => O::LDUMAXB_32_memop,
        ("ldumax", "", "h", 32) => O::LDUMAXH_32_memop,
        ("ldumax", "a", "", 32) => O::LDUMAXA_32_memop,
        ("ldumax", "a", "", 64) => O::LDUMAXA_64_memop,
        ("ldumax", "a", "b", 32) => O::LDUMAXAB_32_memop,
        ("ldumax", "a", "h", 32) => O::LDUMAXAH_32_memop,
        ("ldumax", "l", "", 32) => O::LDUMAXL_32_memop,
        ("ldumax", "l", "", 64) => O::LDUMAXL_64_memop,
        ("ldumax", "l", "b", 32) => O::LDUMAXLB_32_memop,
        ("ldumax", "l", "h", 32) => O::LDUMAXLH_32_memop,
        ("ldumax", "al", "", 32) => O::LDUMAXAL_32_memop,
        ("ldumax", "al", "", 64) => O::LDUMAXAL_64_memop,
        ("ldumax", "al", "b", 32) => O::LDUMAXALB_32_memop,
        ("ldumax", "al", "h", 32) => O::LDUMAXALH_32_memop,
        ("swp", "", "", 32) => O::SWP_32_memop,
        ("swp", "", "", 64) => O::SWP_64_memop,
        ("swp", "", "b", 32) => O::SWPB_32_memop,
        ("swp", "", "h", 32) => O::SWPH_32_memop,
        ("swp", "a", "", 32) => O::SWPA_32_memop,
        ("swp", "a", "", 64) => O::SWPA_64_memop,
        ("swp", "a", "b", 32) => O::SWPAB_32_memop,
        ("swp", "a", "h", 32) => O::SWPAH_32_memop,
        ("swp", "l", "", 32) => O::SWPL_32_memop,
        ("swp", "l", "", 64) => O::SWPL_64_memop,
        ("swp", "l", "b", 32) => O::SWPLB_32_memop,
        ("swp", "l", "h", 32) => O::SWPLH_32_memop,
        ("swp", "al", "", 32) => O::SWPAL_32_memop,
        ("swp", "al", "", 64) => O::SWPAL_64_memop,
        ("swp", "al", "b", 32) => O::SWPALB_32_memop,
        ("swp", "al", "h", 32) => O::SWPALH_32_memop,
        ("cas", "", "", 32) => O::CAS_C32_comswap,
        ("cas", "", "", 64) => O::CAS_C64_comswap,
        ("cas", "", "b", 32) => O::CASB_C32_comswap,
        ("cas", "", "h", 32) => O::CASH_C32_comswap,
        ("cas", "a", "", 32) => O::CASA_C32_comswap,
        ("cas", "a", "", 64) => O::CASA_C64_comswap,
        ("cas", "a", "b", 32) => O::CASAB_C32_comswap,
        ("cas", "a", "h", 32) => O::CASAH_C32_comswap,
        ("cas", "l", "", 32) => O::CASL_C32_comswap,
        ("cas", "l", "", 64) => O::CASL_C64_comswap,
        ("cas", "l", "b", 32) => O::CASLB_C32_comswap,
        ("cas", "l", "h", 32) => O::CASLH_C32_comswap,
        ("cas", "al", "", 32) => O::CASAL_C32_comswap,
        ("cas", "al", "", 64) => O::CASAL_C64_comswap,
        ("cas", "al", "b", 32) => O::CASALB_C32_comswap,
        ("cas", "al", "h", 32) => O::CASALH_C32_comswap,
        ("ldar", "", "", 32) => O::LDAR_LR32_ldstord,
        ("ldar", "", "", 64) => O::LDAR_LR64_ldstord,
        ("ldar", "", "b", 32) => O::LDARB_LR32_ldstord,
        ("ldar", "", "h", 32) => O::LDARH_LR32_ldstord,
        ("stlr", "", "", 32) => O::STLR_SL32_ldstord,
        ("stlr", "", "", 64) => O::STLR_SL64_ldstord,
        ("stlr", "", "b", 32) => O::STLRB_SL32_ldstord,
        ("stlr", "", "h", 32) => O::STLRH_SL32_ldstord,
        _ => panic!("invalid atomic encoding"),
    }
}

#[cfg(test)]
mod logical_immediate_tests {
    use super::logical_immediate;

    // Decode the architectural N:immr:imms fields independently of the
    // planner's run recognition. Enumerate all 8192 field triples per width.
    fn decode(n: u32, r: u32, s: u32, bits: u32) -> Option<u64> {
        let discriminator = (n << 6) | (!s & 63);
        let len = 31u32.checked_sub(discriminator.leading_zeros())?;
        if len < 1 {
            return None;
        }
        let size = 1 << len;
        if size > bits {
            return None;
        }
        let levels = size - 1;
        let ones = (s & levels) + 1;
        if ones == size {
            return None;
        }
        let mask = u64::MAX >> (64 - size);
        let run = (1u64 << ones) - 1;
        let rotation = r & levels;
        let element = if rotation == 0 {
            run
        } else {
            ((run >> rotation) | (run << (size - rotation))) & mask
        };
        let mut value = 0;
        for offset in (0..bits).step_by(size as usize) {
            value |= element << offset;
        }
        Some(value)
    }

    #[test]
    fn every_legal_field_encoding_round_trips_and_invalid_queries_are_total() {
        for bits in [32, 64] {
            for n in 0..=1 {
                for r in 0..64 {
                    for s in 0..64 {
                        if let Some(value) = decode(n, r, s, bits) {
                            let plan = logical_immediate(value, bits).unwrap();
                            assert_eq!(plan.value, value);
                            assert_eq!(decode(plan.n, plan.immr, plan.imms, bits), Some(value));
                            if bits == 32 {
                                assert_eq!(
                                    logical_immediate(value | 0xdeadbeef00000000, bits),
                                    Some(plan)
                                );
                            }
                        }
                    }
                }
            }
            for value in [0, u64::MAX, 0x1234567812345678] {
                assert!(logical_immediate(value, bits).is_none());
            }
        }
        for bits in [0, 1, 8, 16, 31, 33, 65, u32::MAX] {
            assert!(logical_immediate(63, bits).is_none());
        }
    }
}
