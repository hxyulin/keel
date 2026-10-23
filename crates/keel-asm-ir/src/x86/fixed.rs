//! Fixed encodings from the baseline x86/x87 instruction set.
//!
//! These identities describe assembler syntax only. They do not imply IR
//! semantics, user-mode executability, or suitability for compiler selection.
macro_rules! fixed {
    ($($op:ident => ($name:literal, $bytes:expr)),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum FixedOpcode { $($op),* }
        impl FixedOpcode {
            pub const ALL: &'static [Self] = &[$(Self::$op),*];
            pub fn parse(name: &str) -> Option<Self> {
                Some(match name { $($name => Self::$op,)* _ => return None })
            }
            pub const fn mnemonic(self) -> &'static str {
                match self { $(Self::$op => $name),* }
            }
            pub const fn bytes(self) -> &'static [u8] {
                match self { $(Self::$op => $bytes),* }
            }
        }
    }
}
fixed! {
    Fabs => ("fabs", &[0xd9, 0xe1]),
    Fchs => ("fchs", &[0xd9, 0xe0]),
    Ftst => ("ftst", &[0xd9, 0xe4]),
    Fxam => ("fxam", &[0xd9, 0xe5]),
    Fld1 => ("fld1", &[0xd9, 0xe8]),
    Fldl2t => ("fldl2t", &[0xd9, 0xe9]),
    Fldl2e => ("fldl2e", &[0xd9, 0xea]),
    Fldpi => ("fldpi", &[0xd9, 0xeb]),
    Fldlg2 => ("fldlg2", &[0xd9, 0xec]),
    Fldln2 => ("fldln2", &[0xd9, 0xed]),
    F2xm1 => ("f2xm1", &[0xd9, 0xf0]),
    Fyl2x => ("fyl2x", &[0xd9, 0xf1]),
    Fptan => ("fptan", &[0xd9, 0xf2]),
    Fpatan => ("fpatan", &[0xd9, 0xf3]),
    Fxtract => ("fxtract", &[0xd9, 0xf4]),
    Fprem1 => ("fprem1", &[0xd9, 0xf5]),
    Fdecstp => ("fdecstp", &[0xd9, 0xf6]),
    Fincstp => ("fincstp", &[0xd9, 0xf7]),
    Fprem => ("fprem", &[0xd9, 0xf8]),
    Fyl2xp1 => ("fyl2xp1", &[0xd9, 0xf9]),
    Fsqrt => ("fsqrt", &[0xd9, 0xfa]),
    Fsincos => ("fsincos", &[0xd9, 0xfb]),
    Frndint => ("frndint", &[0xd9, 0xfc]),
    Fscale => ("fscale", &[0xd9, 0xfd]),
    Fsin => ("fsin", &[0xd9, 0xfe]),
    Fcos => ("fcos", &[0xd9, 0xff]),
    Fcompp => ("fcompp", &[0xde, 0xd9]),
    Fucompp => ("fucompp", &[0xda, 0xe9]),
    Fnop => ("fnop", &[0xd9, 0xd0]),
    Fnclex => ("fnclex", &[0xdb, 0xe2]),
    Fninit => ("fninit", &[0xdb, 0xe3]),
    Fclex => ("fclex", &[0x9b, 0xdb, 0xe2]),
    Finit => ("finit", &[0x9b, 0xdb, 0xe3]),
    Fwait => ("fwait", &[0x9b]),
    Emms => ("emms", &[0x0f, 0x77]),
    Cpuid => ("cpuid", &[0x0f, 0xa2]),
    Rdtsc => ("rdtsc", &[0x0f, 0x31]),
    Clc => ("clc", &[0xf8]),
    Stc => ("stc", &[0xf9]),
    Cmc => ("cmc", &[0xf5]),
    Cld => ("cld", &[0xfc]),
    Std => ("std", &[0xfd]),
    Pushfq => ("pushfq", &[0x9c]),
    Popfq => ("popfq", &[0x9d]),
    Leavew => ("leavew", &[0x66, 0xc9]),
    Pushfw => ("pushfw", &[0x66, 0x9c]),
    Popfw => ("popfw", &[0x66, 0x9d]),
    Iretw => ("iretw", &[0x66, 0xcf]),
    Iretd => ("iretd", &[0xcf]),
    Rdpmc => ("rdpmc", &[0x0f, 0x33]),
    Sysenter => ("sysenter", &[0x0f, 0x34]),
    // The undefined 64-bit opcode D6 is exposed for intentional traps.
    Udb => ("udb", &[0xd6]),
    // Assemble these forms, but never execute them in a user-mode oracle.
    Cli => ("cli", &[0xfa]),
    Sti => ("sti", &[0xfb]),
    Hlt => ("hlt", &[0xf4]),
    Clts => ("clts", &[0x0f, 0x06]),
    Invd => ("invd", &[0x0f, 0x08]),
    Wbinvd => ("wbinvd", &[0x0f, 0x09]),
    Rdmsr => ("rdmsr", &[0x0f, 0x32]),
    Wrmsr => ("wrmsr", &[0x0f, 0x30]),
    Swapgs => ("swapgs", &[0x0f, 0x01, 0xf8]),
    Iretq => ("iretq", &[0x48, 0xcf]),
    Sysret => ("sysret", &[0x0f, 0x07]),
    Sysretq => ("sysretq", &[0x48, 0x0f, 0x07]),
}
