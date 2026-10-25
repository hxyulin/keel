//! Fixed encodings from the baseline x86/x87 instruction set.
//!
//! These identities describe assembler syntax only. They do not imply IR
//! semantics, user-mode executability, or suitability for compiler selection.
macro_rules! fixed {
    ($($op:ident => $name:literal),* $(,)?) => {
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
                super::isa::fixed::bytes(self)
            }
        }
    }
}
fixed! {
    Fabs => "fabs",
    Fchs => "fchs",
    Ftst => "ftst",
    Fxam => "fxam",
    Fld1 => "fld1",
    Fldl2t => "fldl2t",
    Fldl2e => "fldl2e",
    Fldpi => "fldpi",
    Fldlg2 => "fldlg2",
    Fldln2 => "fldln2",
    F2xm1 => "f2xm1",
    Fyl2x => "fyl2x",
    Fptan => "fptan",
    Fpatan => "fpatan",
    Fxtract => "fxtract",
    Fprem1 => "fprem1",
    Fdecstp => "fdecstp",
    Fincstp => "fincstp",
    Fprem => "fprem",
    Fyl2xp1 => "fyl2xp1",
    Fsqrt => "fsqrt",
    Fsincos => "fsincos",
    Frndint => "frndint",
    Fscale => "fscale",
    Fsin => "fsin",
    Fcos => "fcos",
    Fcompp => "fcompp",
    Fucompp => "fucompp",
    Fnop => "fnop",
    Fnclex => "fnclex",
    Fninit => "fninit",
    Fclex => "fclex",
    Finit => "finit",
    Fwait => "fwait",
    Emms => "emms",
    Cpuid => "cpuid",
    Rdtsc => "rdtsc",
    Clc => "clc",
    Stc => "stc",
    Cmc => "cmc",
    Cld => "cld",
    Std => "std",
    Pushfq => "pushfq",
    Popfq => "popfq",
    Leavew => "leavew",
    Pushfw => "pushfw",
    Popfw => "popfw",
    Iretw => "iretw",
    Iretd => "iretd",
    Rdpmc => "rdpmc",
    Sysenter => "sysenter",
    // The undefined 64-bit opcode D6 is exposed for intentional traps.
    Udb => "udb",
    // Assemble these forms, but never execute them in a user-mode oracle.
    Cli => "cli",
    Sti => "sti",
    Hlt => "hlt",
    Clts => "clts",
    Invd => "invd",
    Wbinvd => "wbinvd",
    Rdmsr => "rdmsr",
    Wrmsr => "wrmsr",
    Swapgs => "swapgs",
    Iretq => "iretq",
    Sysret => "sysret",
    Sysretq => "sysretq",
}
