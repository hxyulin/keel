//! Additional architectural x87 stack, integer-memory, and state operations.
macro_rules! operations {
    ($($variant:ident => $name:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum X87Opcode { $($variant),* }
        impl X87Opcode {
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];
            pub fn parse(name: &str) -> Option<Self> {
                Some(match name { $($name => Self::$variant,)* _ => return None })
            }
            pub const fn mnemonic(self) -> &'static str {
                match self { $(Self::$variant => $name),* }
            }
        }
    }
}
operations! {
    Fst => "fst", Fsub => "fsub", Fsubr => "fsubr", Fdiv => "fdiv", Fdivr => "fdivr",
    Faddp => "faddp", Fdivp => "fdivp", Fdivrp => "fdivrp",
    Fcom => "fcom", Fcomp => "fcomp", Fucom => "fucom", Fucomp => "fucomp",
    Fcomi => "fcomi", Fcomip => "fcomip", Ffree => "ffree", Ffreep => "ffreep",
    Fcmovb => "fcmovb", Fcmove => "fcmove", Fcmovbe => "fcmovbe", Fcmovu => "fcmovu",
    Fcmovne => "fcmovne", Fcmovnbe => "fcmovnbe", Fcmovnu => "fcmovnu",
    Fiadd => "fiadd", Fimul => "fimul", Ficom => "ficom", Ficomp => "ficomp",
    Fisub => "fisub", Fisubr => "fisubr", Fidiv => "fidiv", Fidivr => "fidivr",
    Fist => "fist", Fbld => "fbld", Fbstp => "fbstp",
    Fsave => "fsave", Fnsave => "fnsave", Frstor => "frstor",
    Fldenv => "fldenv", Fstenv => "fstenv", Fnstenv => "fnstenv",
    Fstcw => "fstcw", Fstsw => "fstsw", Fnstsw => "fnstsw",
}

impl X87Opcode {
    pub(super) fn environment(self) -> bool {
        matches!(
            self,
            Self::Fldenv | Self::Fstenv | Self::Fnstenv | Self::Fsave | Self::Fnsave | Self::Frstor
        )
    }
    pub(super) fn att_memory(name: &str) -> Option<(Self, u16)> {
        let (base, bits) = if let Some(base) = name.strip_suffix('s') {
            (base, 16)
        } else if let Some(base) = name.strip_suffix('l') {
            (base, 32)
        } else {
            return None;
        };
        let op = Self::parse(base)?;
        let width = match op {
            Self::Fst
            | Self::Fsub
            | Self::Fsubr
            | Self::Fdiv
            | Self::Fdivr
            | Self::Fcom
            | Self::Fcomp => {
                if bits == 16 {
                    32
                } else {
                    64
                }
            }
            Self::Fiadd
            | Self::Fimul
            | Self::Ficom
            | Self::Ficomp
            | Self::Fisub
            | Self::Fisubr
            | Self::Fidiv
            | Self::Fidivr
            | Self::Fist
            | Self::Fldenv
            | Self::Fstenv
            | Self::Fnstenv
            | Self::Fsave
            | Self::Fnsave
            | Self::Frstor => bits,
            _ => return None,
        };
        Some((op, width))
    }

    pub(super) fn from_att(self, args: &[super::Operand]) -> Self {
        use super::{Operand, RegisterClass};
        // GAS/LLVM retain the historical reversed subtraction/division spelling
        // for ST(i)-destination forms. The shared model uses Intel semantics.
        let other_destination = matches!(args, [Operand::Register(a), Operand::Register(b)]
            if a.class == RegisterClass::X87 && a.number != 0
                && b.class == RegisterClass::X87 && b.number == 0);
        match self {
            Self::Fdivp => Self::Fdivrp,
            Self::Fdivrp => Self::Fdivp,
            Self::Fsub if other_destination => Self::Fsubr,
            Self::Fsubr if other_destination => Self::Fsub,
            Self::Fdiv if other_destination => Self::Fdivr,
            Self::Fdivr if other_destination => Self::Fdiv,
            other => other,
        }
    }
}

impl super::Instruction {
    pub(super) fn x87_data16(mut self) -> Result<Self, String> {
        if !self.lock && matches!(self.opcode, super::Opcode::X87(op) if op.environment()) {
            if let [super::Operand::Memory(m)] = self.operands.as_mut_slice() {
                if m.width == 0 {
                    m.width = 16;
                    return Ok(self);
                }
            }
        }
        Err("data16 requires an unsized x87 environment/state memory operand".into())
    }
}
