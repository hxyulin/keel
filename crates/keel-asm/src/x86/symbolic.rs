//! Symbolic instructions, encoded once at collection. Their bytes only change
//! when a short branch widens or an operand folds to an absolute constant, so
//! layout passes copy the cached encoding and fixups. A compact operand
//! record with interned symbols rebuilds the instruction for those cases.
use super::{Expr, SymId, Symbols, encode};
use keel_asm_ir::{
    Expression,
    x86::{self, Address, Memory, Modifier, Operand, Register, RegisterClass},
};

/// A fixup of a cached encoding, relative to the instruction's start, in 24
/// bytes: symbols are ids (or [`NONE`]) and the addend of an encoded field
/// fits 64 bits (an instruction whose addend does not stays uncached).
pub(super) struct CachedFix {
    add: SymId,
    subtract: SymId,
    constant: i64,
    pub offset: u8,
    pub width: u8,
    pub kind: u8,
    pub modifier: Modifier,
}
const _: () = assert!(std::mem::size_of::<CachedFix>() == 24);
impl CachedFix {
    pub fn expr(&self) -> Expr {
        Expr {
            add: (self.add != NONE).then_some(self.add),
            subtract: (self.subtract != NONE).then_some(self.subtract),
            constant: self.constant.into(),
        }
    }
    /// Whether this operand may fold to a constant on some pass: only a
    /// difference, or an alias once one exists, can be absolute.
    pub fn may_fold(&self, aliases: usize) -> bool {
        self.modifier == Modifier::None
            && self.add != NONE
            && (aliases != 0 || self.subtract != NONE)
    }
    fn candidate(&self) -> bool {
        self.modifier == Modifier::None && self.add != NONE
    }
}
#[derive(Clone, Copy)]
pub(super) struct Symbolic {
    /// `bytes[bytes..bytes + len]` and `fixups[fixups..fixups + count]`.
    bytes: u32,
    fixups: u32,
    len: u8,
    count: u8,
    /// Whether the cache holds this instruction's encoding with `short`. If
    /// not, layout reencodes it every pass and reports its error.
    cached: bool,
    /// A direct jump still assumed to fit rel8.
    pub short: bool,
    lock: bool,
    opcode: x86::Opcode,
    /// The operand record in `operands`, absent when never needed again.
    operands: u32,
}
const NONE: u32 = u32::MAX;

/// Cached encodings and operand records of all symbolic instructions.
#[derive(Default)]
pub(super) struct Cache {
    pub entries: Vec<Symbolic>,
    bytes: Vec<u8>,
    fixups: Vec<CachedFix>,
    operands: Vec<u8>,
}
impl Cache {
    /// Record a symbolic instruction; returns its index.
    pub fn push(&mut self, i: &x86::Instruction, symbols: &mut Symbols) -> u32 {
        let short = matches!(i.opcode, x86::Opcode::Jmp | x86::Opcode::Jcc(_))
            && matches!(i.operands.as_slice(), [Operand::Immediate(address)]
                if address.modifier == Modifier::None && address.expression.add.is_some()
                    && address.expression.subtract.is_none());
        let mut s = Symbolic {
            bytes: 0,
            fixups: 0,
            len: 0,
            count: 0,
            cached: false,
            short,
            lock: i.lock,
            opcode: i.opcode,
            operands: NONE,
        };
        self.fill(&mut s, i, symbols);
        // Folding is checked against the cached fixups: that needs exactly
        // one fixup, in order, for every operand that may fold.
        if s.cached {
            let candidates = i
                .operands
                .iter()
                .filter_map(|o| match o {
                    Operand::Immediate(a) => Some(a),
                    Operand::Memory(m) => Some(&m.displacement),
                    Operand::Register(_) => None,
                })
                .filter(|a| a.modifier == Modifier::None && a.expression.add.is_some());
            let fixups = self.fixups(&s).iter().filter(|f| f.candidate());
            // Compare symbols only: the encoder adjusts PC-relative constants.
            s.cached = candidates
                .map(|a| {
                    let e = symbols.expr(&a.expression);
                    (e.add, e.subtract)
                })
                .eq(fixups.map(|f| {
                    let e = f.expr();
                    (e.add, e.subtract)
                }));
        }
        let needed = !s.cached
            || s.short
            || self.fixups(&s).iter().any(CachedFix::candidate);
        if needed {
            s.operands = u32::try_from(self.operands.len()).expect("operand records exceed 4 GiB");
            self.write_operands(&i.operands, symbols);
        }
        let index = u32::try_from(self.entries.len()).expect("too many symbolic instructions");
        self.entries.push(s);
        index
    }
    /// Encode `i` with the entry's `short` into the cache.
    fn fill(&mut self, s: &mut Symbolic, i: &x86::Instruction, symbols: &mut Symbols) {
        s.cached = false;
        let Ok(e) = encode::encode(i, s.short) else {
            return;
        };
        let (Ok(len), Ok(count)) = (u8::try_from(e.bytes.len()), u8::try_from(e.fixups.len()))
        else {
            return;
        };
        s.bytes = u32::try_from(self.bytes.len()).expect("instruction cache exceeds 4 GiB");
        s.len = len;
        self.bytes.extend_from_slice(&e.bytes);
        s.fixups = u32::try_from(self.fixups.len()).expect("instruction cache exceeds 4 GiB");
        s.count = count;
        for f in e.fixups {
            let expr = symbols.expr(&f.address.expression);
            let Ok(constant) = i64::try_from(expr.constant) else {
                return;
            };
            self.fixups.push(CachedFix {
                add: expr.add.unwrap_or(NONE),
                subtract: expr.subtract.unwrap_or(NONE),
                constant,
                // Within one instruction, and of the encoder's few kinds.
                offset: f.offset as u8,
                width: f.width,
                kind: f.kind as u8,
                modifier: f.address.modifier,
            });
        }
        s.cached = true;
    }
    pub fn cached(&self, index: u32) -> Option<(&[u8], &[CachedFix])> {
        let s = &self.entries[index as usize];
        s.cached.then(|| (self.bytes(s), self.fixups(s)))
    }
    fn bytes(&self, s: &Symbolic) -> &[u8] {
        &self.bytes[s.bytes as usize..s.bytes as usize + usize::from(s.len)]
    }
    fn fixups(&self, s: &Symbolic) -> &[CachedFix] {
        &self.fixups[s.fixups as usize..s.fixups as usize + usize::from(s.count)]
    }
    /// The instruction, rebuilt from its operand record.
    pub fn instruction(&self, index: u32, symbols: &Symbols) -> x86::Instruction {
        let s = &self.entries[index as usize];
        let mut r = Reader {
            bytes: &self.operands[s.operands as usize..],
            symbols,
        };
        let count = r.u8();
        x86::Instruction {
            opcode: s.opcode,
            operands: (0..count).map(|_| r.operand()).collect(),
            lock: s.lock,
        }
    }
    /// Widen a short branch, reencoding its cache.
    pub fn widen(&mut self, index: u32, symbols: &mut Symbols) {
        let i = self.instruction(index, symbols);
        let mut s = self.entries[index as usize];
        s.short = false;
        self.fill(&mut s, &i, symbols);
        self.entries[index as usize] = s;
    }
    fn write_operands(&mut self, operands: &[Operand], symbols: &mut Symbols) {
        let out = &mut self.operands;
        out.push(u8::try_from(operands.len()).expect("too many operands"));
        for o in operands {
            match o {
                Operand::Register(r) => {
                    out.push(0);
                    register(out, r);
                }
                Operand::Immediate(a) => {
                    out.push(1);
                    address(out, a, symbols);
                }
                Operand::Memory(m) => {
                    out.push(2);
                    out.extend_from_slice(&m.width.to_le_bytes());
                    out.push(
                        u8::from(m.base.is_some())
                            | u8::from(m.index.is_some()) << 1
                            | u8::from(m.segment.is_some()) << 2,
                    );
                    if let Some(r) = &m.base {
                        register(out, r);
                    }
                    if let Some(r) = &m.index {
                        register(out, r);
                    }
                    out.push(m.scale);
                    if let Some(segment) = m.segment {
                        out.push(segment);
                    }
                    address(out, &m.displacement, symbols);
                }
            }
        }
    }
}
fn register(out: &mut Vec<u8>, r: &Register) {
    out.push(match r.class {
        RegisterClass::Gpr => 0,
        RegisterClass::Xmm => 1,
        RegisterClass::X87 => 2,
        RegisterClass::Rip => 3,
    });
    out.push(r.number);
    out.extend_from_slice(&r.width.to_le_bytes());
    out.push(u8::from(r.high_byte));
}
const MODIFIERS: [Modifier; 6] = [
    Modifier::None,
    Modifier::Plt,
    Modifier::GotPcRel,
    Modifier::TpOff,
    Modifier::GotTpOff,
    Modifier::DtpOff,
];
fn address(out: &mut Vec<u8>, a: &Address, symbols: &mut Symbols) {
    let e = symbols.expr(&a.expression);
    let modifier = MODIFIERS.iter().position(|&m| m == a.modifier).unwrap() as u8;
    out.push(
        modifier
            | u8::from(e.add.is_some()) << 3
            | u8::from(e.subtract.is_some()) << 4
            | u8::from(e.constant != 0) << 5,
    );
    for id in e.add.into_iter().chain(e.subtract) {
        out.extend_from_slice(&id.to_le_bytes());
    }
    if e.constant != 0 {
        out.extend_from_slice(&e.constant.to_le_bytes());
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    symbols: &'a Symbols,
}
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let (head, rest) = self.bytes.split_first_chunk::<N>().unwrap();
        self.bytes = rest;
        *head
    }
    fn u8(&mut self) -> u8 {
        self.take::<1>()[0]
    }
    fn register(&mut self) -> Register {
        let class = match self.u8() {
            0 => RegisterClass::Gpr,
            1 => RegisterClass::Xmm,
            2 => RegisterClass::X87,
            _ => RegisterClass::Rip,
        };
        let number = self.u8();
        let width = u16::from_le_bytes(self.take());
        Register {
            class,
            number,
            width,
            high_byte: self.u8() != 0,
        }
    }
    fn symbol(&mut self) -> String {
        let id: SymId = u32::from_le_bytes(self.take());
        self.symbols.name(id).into()
    }
    fn address(&mut self) -> Address {
        let flags = self.u8();
        let add = (flags & 8 != 0).then(|| self.symbol());
        let subtract = (flags & 16 != 0).then(|| self.symbol());
        let constant = if flags & 32 != 0 {
            i128::from_le_bytes(self.take())
        } else {
            0
        };
        Address {
            expression: Expression {
                add,
                subtract,
                constant,
            },
            modifier: MODIFIERS[usize::from(flags & 7)],
        }
    }
    fn operand(&mut self) -> Operand {
        match self.u8() {
            0 => Operand::Register(self.register()),
            1 => Operand::Immediate(self.address()),
            _ => {
                let width = u16::from_le_bytes(self.take());
                let present = self.u8();
                let base = (present & 1 != 0).then(|| self.register());
                let index = (present & 2 != 0).then(|| self.register());
                let scale = self.u8();
                let segment = (present & 4 != 0).then(|| self.u8());
                Operand::Memory(Memory {
                    width,
                    base,
                    index,
                    scale,
                    segment,
                    displacement: self.address(),
                })
            }
        }
    }
}
