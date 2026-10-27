//! Inline assembly: a template of target instructions with `%0`, `%1`…
//! standing for operands, and a constraint string, in the style GCC and
//! LLVM share, saying what each operand is:
//!
//! ```text
//! v2 = asm "add %0, %1, %2", "=r,r,r"(v0, v1) -> i64
//! v3 = asm "ldr %0, %1", "=r,m,~{memory}"(v0) -> i64
//! ```
//!
//! Bracketed numeric references (`%[0]`, `%c[1]`) delimit an operand
//! number when literal digits immediately follow the substitution.
//!
//! Constraints are comma-separated: outputs (`=`) first, one per result,
//! then inputs, one per argument, then clobbers (`~`). Operands are numbered
//! in that order, outputs first. What a register name looks like in the
//! template (`x0` or `w0`) is the target's business.

use crate::Type;
use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct InlineAsm {
    pub template: String,
    /// Output indices whose registers are written before inputs are consumed.
    pub early_clobbers: Vec<u16>,
    /// Each output and the type of its result.
    pub outputs: Vec<(AsmOperand, Type)>,
    pub inputs: Vec<AsmOperand>,
    pub clobbers: Vec<AsmClobber>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AsmOperand {
    /// `r`: a general-purpose register.
    Reg,
    /// `w`: a SIMD/FP register.
    FloatReg,
    /// `{x0}`, `{d8}`: that register.
    Fixed(PhysReg),
    /// `m`: memory; the operand is its address. Inputs only.
    Mem,
    /// `mwrite`: a write-only object, supplied by its address. No SSA result.
    MemWrite,
    /// `mreadwrite`: a read/write object, supplied by its address. No SSA result.
    MemReadWrite,
    /// `i`: a numeric constant or symbol address. Inputs only.
    Imm,
    /// `n`: a numeric constant, excluding symbol addresses. Inputs only.
    NumericImm,
    /// `ri`: an immediate when legal, otherwise a GPR. X86 inputs only.
    RegImm,
    /// `re`: a signed 32-bit numeric immediate, otherwise a GPR. X86 inputs only.
    RegSignedImm32,
    /// `rI`: a numeric immediate in 0..=31, otherwise a GPR. X86 inputs only.
    RegShiftImm5,
    /// `Nd`: an unsigned byte immediate, otherwise RDX. X86 inputs only.
    PortImm8,
    /// `rsp`: one balanced x86 stack dependency, supplied by an `i64`
    /// snapshot from `StackSave` followed by `PtrToInt`. Inputs only, with no
    /// SSA output, tie or writeback. The backend renders live physical RSP;
    /// it never copies this argument's numeric value into RSP. Evaluating
    /// other inputs can change the live stack after the snapshot.
    ///
    /// The template may change RSP temporarily, but must restore the same
    /// stack/frame state and return normally. Calls obey the ABI, including
    /// its SIMD clobbers. This contract does not model a persistent stack
    /// switch, terminal transfer or unwinding through the template.
    StackPointer,
    /// `@ccz`, etc.: x86 flags captured as a boolean integer result.
    Flag(u8),
    /// `0`, `1`…: in the same register as that output, which it initializes.
    /// Inputs only. Scalar GPR ties may use different input/result widths;
    /// each operand retains its own type and register view. This does not
    /// promise an extension of the input into unspecified upper bits.
    /// Multiple inputs may initialize one output only when their common low
    /// bits are proven equal by `same_register_input`. The widest input owns
    /// initialization. An explicit tie may initialize an early-clobber output;
    /// an independent fixed input may overlap only a non-early output on x86.
    Tied(u16),
}

/// A register by class and number: `x5` and `w5` are the general-purpose
/// register 5, `v5`, `d5` and `s5` the SIMD/FP register 5. x86 GPR
/// views such as rax/eax/al share the generated physical register number.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PhysReg {
    pub arch: AsmArch,
    pub float: bool,
    pub num: u8,
}

/// Architecture of an explicitly named register.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AsmArch {
    Aarch64,
    X86,
}

/// What the template changes besides its outputs.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AsmClobber {
    Reg(PhysReg),
    /// `~{memory}`: memory the operands do not name.
    Memory,
    /// `~{cc}`: the condition flags.
    Flags,
}

impl InlineAsm {
    /// Builds inline assembly from a constraint string; `output_types` are
    /// the result types, one per output.
    pub fn new(template: &str, constraints: &str, output_types: &[Type]) -> Result<Self, String> {
        let mut asm = InlineAsm {
            template: template.to_string(),
            early_clobbers: Vec::new(),
            outputs: Vec::new(),
            inputs: Vec::new(),
            clobbers: Vec::new(),
        };
        let mut output_kinds = Vec::new();
        for part in constraints.split(',').filter(|p| !p.is_empty()) {
            if let Some(name) = part.strip_prefix('~') {
                let name =
                    braced(name).ok_or_else(|| format!("expected `~{{name}}`, got `{part}`"))?;
                asm.clobbers.push(match name {
                    "memory" => AsmClobber::Memory,
                    "cc" => AsmClobber::Flags,
                    _ => AsmClobber::Reg(PhysReg::parse(name)?),
                });
            } else if !asm.clobbers.is_empty() {
                return Err(format!("`{part}` comes after a clobber; clobbers go last"));
            } else if let Some(code) = part.strip_prefix('=') {
                if !asm.inputs.is_empty() {
                    return Err(format!("output `{part}` comes after an input"));
                }
                let code = if let Some(code) = code.strip_prefix('&') {
                    asm.early_clobbers.push(
                        u16::try_from(output_kinds.len()).map_err(|_| "too many asm outputs")?,
                    );
                    code
                } else {
                    code
                };
                let operand = operand(code)?;
                if operand == AsmOperand::StackPointer {
                    return Err("rsp stack dependency is input-only".into());
                }
                if !matches!(
                    operand,
                    AsmOperand::Reg
                        | AsmOperand::FloatReg
                        | AsmOperand::Fixed(_)
                        | AsmOperand::Flag(_)
                ) {
                    return Err(format!("output `{part}` must be a register"));
                }
                if matches!(operand, AsmOperand::Flag(_)) && part.starts_with("=&") {
                    return Err("flag outputs cannot be early-clobber".into());
                }
                output_kinds.push(operand);
            } else {
                let op = operand(part)?;
                if matches!(op, AsmOperand::Flag(_)) {
                    return Err("flag constraints require an output".into());
                }
                asm.inputs.push(op);
            }
        }
        if output_kinds.len() != output_types.len() {
            return Err(format!(
                "{} output constraint(s) but {} result type(s)",
                output_kinds.len(),
                output_types.len()
            ));
        }
        asm.outputs = output_kinds
            .into_iter()
            .zip(output_types.iter().copied())
            .collect();
        asm.stack_pointer_input()?;
        Ok(asm)
    }

    /// Returns the unique `rsp` input index, rejecting malformed stack
    /// dependencies and conflicting fixed/clobbered RSP aliases.
    ///
    /// Input types and capture provenance need the enclosing function; the
    /// verifier checks them with [`is_stack_pointer_snapshot`]. Architecture
    /// legality belongs to the target backend.
    pub fn stack_pointer_input(&self) -> Result<Option<usize>, String> {
        if self
            .outputs
            .iter()
            .any(|(op, _)| *op == AsmOperand::StackPointer)
        {
            return Err("rsp stack dependency is input-only".into());
        }
        let mut inputs = self
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(i, op)| (*op == AsmOperand::StackPointer).then_some(i));
        let Some(index) = inputs.next() else {
            return Ok(None);
        };
        if inputs.next().is_some() {
            return Err("duplicate rsp stack dependency".into());
        }
        let stack_register = |r: PhysReg| r.arch == AsmArch::X86 && !r.float && r.num == 4;
        if self
            .outputs
            .iter()
            .map(|(op, _)| op)
            .chain(&self.inputs)
            .any(|op| matches!(op, AsmOperand::Fixed(r) if stack_register(*r)))
            || self
                .clobbers
                .iter()
                .any(|c| matches!(c, AsmClobber::Reg(r) if stack_register(*r)))
        {
            return Err(
                "rsp stack dependency conflicts with a fixed or clobbered stack register".into(),
            );
        }
        Ok(Some(index))
    }

    /// The constraint string, as [`InlineAsm::new`] reads it.
    pub fn constraints(&self) -> String {
        let outputs = self.outputs.iter().enumerate().map(|(i, (op, _))| {
            let early = if self.early_clobbers.contains(&(i as u16)) {
                "&"
            } else {
                ""
            };
            format!("={early}{op}")
        });
        let inputs = self.inputs.iter().map(|op| op.to_string());
        let clobbers = self.clobbers.iter().map(|c| match c {
            AsmClobber::Reg(r) => format!("~{{{r}}}"),
            AsmClobber::Memory => "~{memory}".into(),
            AsmClobber::Flags => "~{cc}".into(),
        });
        outputs
            .chain(inputs)
            .chain(clobbers)
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// `name` from `{name}`.
fn braced(s: &str) -> Option<&str> {
    s.strip_prefix('{')?.strip_suffix('}')
}

fn operand(code: &str) -> Result<AsmOperand, String> {
    Ok(match code {
        "r" => AsmOperand::Reg,
        "w" => AsmOperand::FloatReg,
        "m" => AsmOperand::Mem,
        "mwrite" => AsmOperand::MemWrite,
        "mreadwrite" => AsmOperand::MemReadWrite,
        "i" => AsmOperand::Imm,
        "n" => AsmOperand::NumericImm,
        "ri" => AsmOperand::RegImm,
        "re" => AsmOperand::RegSignedImm32,
        "rI" => AsmOperand::RegShiftImm5,
        "Nd" => AsmOperand::PortImm8,
        "rsp" => AsmOperand::StackPointer,
        _ if code.starts_with("@cc") => AsmOperand::Flag(
            keel_asm_ir::x86::isa::parse_condition_code(&code[3..])
                .ok_or_else(|| format!("unknown x86 flag condition `{code}`"))?,
        ),
        _ if code.starts_with('{') => AsmOperand::Fixed(PhysReg::parse(
            braced(code).ok_or_else(|| format!("unclosed register in `{code}`"))?,
        )?),
        _ => AsmOperand::Tied(
            code.parse()
                .map_err(|_| format!("unknown constraint `{code}`"))?,
        ),
    })
}

impl PhysReg {
    /// `x0`–`x30` and `w0`–`w30` (also `fp` and `lr`), or `v0`–`v31` and
    /// their `q`, `d`, `s`, `h` and `b` views. Also accepts generated x86 GPR
    /// names and aliases, retaining their architecture and canonical number.
    pub fn parse(name: &str) -> Result<Self, String> {
        if let Some(r) = keel_asm_ir::x86::Register::parse(name)
            && r.class == keel_asm_ir::x86::RegisterClass::Gpr
        {
            return Ok(Self {
                arch: AsmArch::X86,
                float: false,
                // AH/CH/DH/BH encode as 4..7 but alias GPRs 0..3.
                num: if r.high_byte { r.number - 4 } else { r.number },
            });
        }
        let bad = || format!("unknown register `{name}`");
        match name {
            "fp" => {
                return Ok(PhysReg {
                    arch: AsmArch::Aarch64,
                    float: false,
                    num: 29,
                });
            }
            "lr" => {
                return Ok(PhysReg {
                    arch: AsmArch::Aarch64,
                    float: false,
                    num: 30,
                });
            }
            _ => {}
        }
        let (class, num) = name.split_at_checked(1).ok_or_else(bad)?;
        let num: u8 = num.parse().map_err(|_| bad())?;
        let float = match class {
            "x" | "w" if num <= 30 => false,
            "v" | "q" | "d" | "s" | "h" | "b" if num <= 31 => true,
            _ => return Err(bad()),
        };
        Ok(PhysReg {
            arch: AsmArch::Aarch64,
            float,
            num,
        })
    }
}

impl fmt::Display for PhysReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.arch == AsmArch::X86 {
            let r = keel_asm_ir::x86::Register {
                class: keel_asm_ir::x86::RegisterClass::Gpr,
                number: self.num,
                width: 64,
                high_byte: false,
            };
            return write!(f, "{r}");
        }
        let class = if self.float { 'v' } else { 'x' };
        write!(f, "{class}{}", self.num)
    }
}

impl fmt::Display for AsmOperand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AsmOperand::Reg => f.write_str("r"),
            AsmOperand::FloatReg => f.write_str("w"),
            AsmOperand::Fixed(r) => write!(f, "{{{r}}}"),
            AsmOperand::Mem => f.write_str("m"),
            AsmOperand::MemWrite => f.write_str("mwrite"),
            AsmOperand::MemReadWrite => f.write_str("mreadwrite"),
            AsmOperand::Imm => f.write_str("i"),
            AsmOperand::NumericImm => f.write_str("n"),
            AsmOperand::RegImm => f.write_str("ri"),
            AsmOperand::RegSignedImm32 => f.write_str("re"),
            AsmOperand::RegShiftImm5 => f.write_str("rI"),
            AsmOperand::PortImm8 => f.write_str("Nd"),
            AsmOperand::StackPointer => f.write_str("rsp"),
            AsmOperand::Flag(cc) => match keel_asm_ir::x86::isa::condition_code_name(*cc) {
                Some(name) => write!(f, "@cc{name}"),
                None => write!(f, "@cc({cc})"),
            },
            AsmOperand::Tied(n) => write!(f, "{n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_flags_and_optional_immediates_round_trip() {
        let asm = InlineAsm::new("", "=@ccz,=@ccpo,ri,re,rI", &[Type::I8, Type::I64]).unwrap();
        assert_eq!(
            asm.outputs,
            [
                (AsmOperand::Flag(4), Type::I8),
                (AsmOperand::Flag(11), Type::I64)
            ]
        );
        assert_eq!(
            asm.inputs,
            [
                AsmOperand::RegImm,
                AsmOperand::RegSignedImm32,
                AsmOperand::RegShiftImm5
            ]
        );
        assert_eq!(asm.constraints(), "=@cce,=@ccnp,ri,re,rI");
        assert_eq!(
            asm,
            InlineAsm::new("", &asm.constraints(), &[Type::I8, Type::I64]).unwrap()
        );
        for (constraints, types, expected) in [
            ("=@ccwat", &[Type::I64][..], "unknown x86 flag condition"),
            ("=&@ccz", &[Type::I64][..], "early-clobber"),
            ("@ccz", &[][..], "require an output"),
            ("=ri", &[Type::I64][..], "must be a register"),
        ] {
            assert!(
                InlineAsm::new("", constraints, types)
                    .unwrap_err()
                    .contains(expected)
            );
        }
    }

    #[test]
    fn constraints_round_trip() {
        let asm = InlineAsm::new(
            "",
            "=r,=&{w3},r,{d1},m,n,0,~{lr},~{memory},~{cc}",
            &[Type::I64, Type::I32],
        )
        .unwrap();
        assert_eq!(
            asm.constraints(),
            "=r,=&{x3},r,{v1},m,n,0,~{x30},~{memory},~{cc}"
        );
        let again = InlineAsm::new("", &asm.constraints(), &[Type::I64, Type::I32]).unwrap();
        assert_eq!(asm, again);
    }

    #[test]
    fn x86_register_aliases_and_early_clobbers_round_trip() {
        let asm = InlineAsm::new("", "=&{eax},{rdx},0,~{r11d}", &[Type::I64]).unwrap();
        assert_eq!(asm.constraints(), "=&{rax},{rdx},0,~{r11}");
        assert_eq!(asm.early_clobbers, [0]);
        assert_eq!(
            asm,
            InlineAsm::new("", &asm.constraints(), &[Type::I64]).unwrap()
        );
        assert_eq!(
            PhysReg::parse("ah").unwrap(),
            PhysReg::parse("rax").unwrap()
        );
        assert_ne!(
            PhysReg::parse("x0").unwrap(),
            PhysReg::parse("rax").unwrap()
        );
    }

    #[test]
    fn misplaced_or_unknown_constraints() {
        let err = |c: &str, n: usize| InlineAsm::new("", c, &vec![Type::I64; n]).unwrap_err();
        assert!(err("r,=r", 1).contains("after an input"));
        assert!(err("~{memory},r", 0).contains("clobbers go last"));
        assert!(err("=m", 1).contains("must be a register"));
        assert!(err("=r", 0).contains("1 output constraint(s) but 0"));
        assert!(err("q", 0).contains("unknown constraint"));
        assert!(err("{x31}", 0).contains("unknown register"));
        assert!(err("~{unknown}", 0).contains("unknown register"));
    }
}

/// Proves that two simultaneous inputs can initialize one scalar register.
/// Only the common low bits matter. SSA aliases, lossless low-bit views,
/// numeric constants and equal symbolic addresses establish equality. Distinct
/// loads, arithmetic and unrelated block parameters do not. In particular,
/// equal source spelling or equal allocated homes never establish equality.
/// This is conservative: failure also means equality was not proven.
pub fn same_register_input(func: &crate::Function, a: crate::Value, b: crate::Value) -> bool {
    use crate::{ConvertOp, InstData, Type, ValueDef};
    let a = func.resolve(a);
    let b = func.resolve(b);
    if a == b {
        return true;
    }
    let scalar = |ty: Type| (ty.is_int() && ty != Type::I128) || ty == Type::Ptr;
    if !scalar(func.value_type(a)) || !scalar(func.value_type(b)) {
        return false;
    }
    let bits = func
        .value_type(a)
        .bytes()
        .unwrap_or(8)
        .min(func.value_type(b).bytes().unwrap_or(8))
        * 8;
    let low_view = |mut value| {
        // Bound traversal even for malformed, cyclic IR supplied to a backend.
        for _ in 0..64 {
            let ValueDef::Result(inst, 0) = func.value_def(value) else {
                break;
            };
            let InstData::Convert { op, arg, .. } = func.inst_data(inst) else {
                break;
            };
            if !matches!(
                op,
                ConvertOp::Trunc
                    | ConvertOp::Uext
                    | ConvertOp::Sext
                    | ConvertOp::PtrToInt
                    | ConvertOp::IntToPtr
                    | ConvertOp::Bitcast
            ) {
                break;
            }
            let arg = func.resolve(*arg);
            if !scalar(func.value_type(arg)) || func.value_type(arg).bytes().unwrap_or(8) * 8 < bits
            {
                break;
            }
            value = arg;
        }
        value
    };
    let a = low_view(a);
    let b = low_view(b);
    if a == b {
        return true;
    }
    let constant = |value| {
        let ValueDef::Result(inst, 0) = func.value_def(value) else {
            return None;
        };
        match func.inst_data(inst) {
            InstData::Iconst { imm, .. } => Some(*imm as u64),
            _ => None,
        }
    };
    if let (Some(a), Some(b)) = (constant(a), constant(b)) {
        let mask = u64::MAX >> (64 - bits);
        return a & mask == b & mask;
    }
    matches!((symbolic_immediate(func, a), symbolic_immediate(func, b)),
        (Some(a), Some(b)) if a == b)
}

/// Whether `value` is the canonical `i64` stack-dependency snapshot:
/// `PtrToInt(StackSave())`. Value aliases are resolved at both steps. Block
/// parameters, loads, arithmetic and invented integer values do not establish
/// a stack observation. This proves capture provenance, not the current RSP
/// value or the template's balanced-stack promise.
pub fn is_stack_pointer_snapshot(func: &crate::Function, value: crate::Value) -> bool {
    use crate::{ConvertOp, InstData, Intrinsic, ValueDef};
    let value = func.resolve(value);
    if func.value_type(value) != Type::I64 {
        return false;
    }
    let ValueDef::Result(convert, 0) = func.value_def(value) else {
        return false;
    };
    let InstData::Convert {
        op: ConvertOp::PtrToInt,
        ty: Type::I64,
        arg,
    } = func.inst_data(convert)
    else {
        return false;
    };
    let pointer = func.resolve(*arg);
    if func.value_type(pointer) != Type::Ptr {
        return false;
    }
    let ValueDef::Result(capture, 0) = func.value_def(pointer) else {
        return false;
    };
    matches!(func.inst_data(capture), InstData::Intrinsic { id: Intrinsic::StackSave, args } if args.is_empty())
}

/// Resolve the relocatable subset accepted by the `i` constraint. An address
/// remains one symbol plus a checked signed byte addend; loads are never folded.
pub fn symbolic_immediate(
    func: &crate::Function,
    value: crate::Value,
) -> Option<(crate::Symbol, i64)> {
    fn walk(f: &crate::Function, v: crate::Value, depth: usize) -> Option<(crate::Symbol, i64)> {
        use crate::{BinaryOp, ConvertOp, InstData, ValueDef};
        if depth == 64 {
            return None;
        }
        let ValueDef::Result(i, _) = f.value_def(f.resolve(v)) else {
            return None;
        };
        match f.inst_data(i) {
            InstData::SymbolAddr { symbol } => Some((*symbol, 0)),
            InstData::Convert {
                op: ConvertOp::PtrToInt | ConvertOp::IntToPtr,
                arg,
                ..
            } => walk(f, *arg, depth + 1),
            InstData::Binary {
                op: BinaryOp::Ptradd | BinaryOp::Iadd | BinaryOp::Isub,
                args,
                ..
            } => {
                let number = |v| {
                    let ValueDef::Result(i, _) = f.value_def(f.resolve(v)) else {
                        return None;
                    };
                    if let InstData::Iconst { imm, .. } = f.inst_data(i) {
                        Some(*imm)
                    } else {
                        None
                    }
                };
                let (symbol, addend) = walk(f, args[0], depth + 1)?;
                let offset = number(args[1])?;
                let addend = if matches!(
                    f.inst_data(i),
                    InstData::Binary {
                        op: BinaryOp::Isub,
                        ..
                    }
                ) {
                    addend.checked_sub(offset)?
                } else {
                    addend.checked_add(offset)?
                };
                Some((symbol, addend))
            }
            _ => None,
        }
    }
    walk(func, value, 0)
}

#[cfg(test)]
mod immediate_tests {
    #[test]
    fn final_verification_distinguishes_symbols_numbers_and_runtime_values() {
        for (constraint, operand, good) in [
            ("i", "v2", true),
            ("n", "v2", false),
            ("n", "v1", true),
            ("i", "v0", false),
        ] {
            let m = crate::parse(&format!("decl data @anchor\nfunc @f(i64) {{\nblock0(v0: i64):\n v1 = iconst.i64 7\n v2 = symbol_addr @anchor\n asm \"\", \"{constraint}\"({operand})\n return\n}}\n")).unwrap();
            assert_eq!(
                crate::verify_module(&m).is_ok(),
                good,
                "{constraint} {operand}"
            );
        }
    }

    #[test]
    fn symbolic_addends_are_checked_without_folding_loads_or_overflow() {
        let m = crate::parse("decl data @anchor\nfunc @f() {\nblock0:\n v0 = symbol_addr @anchor\n v1 = iconst.i64 -7\n v2 = ptradd v0, v1\n v3 = iconst.i64 -9223372036854775808\n v4 = ptradd v2, v3\n v5 = load.ptr v0\n return\n}\n").unwrap();
        let f = &m.functions[0];
        let values: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| f.inst_results(i).first().copied())
            .collect();
        assert_eq!(super::symbolic_immediate(f, values[2]).unwrap().1, -7);
        assert_eq!(super::symbolic_immediate(f, values[4]), None);
        assert_eq!(super::symbolic_immediate(f, values[5]), None);
    }
}

#[cfg(test)]
mod memory_tests {
    #[test]
    fn address_operands_round_trip_without_memory_ssa_results() {
        let asm =
            super::InlineAsm::new("", "=r,m,mwrite,mreadwrite,~{cc}", &[crate::Type::I64]).unwrap();
        assert_eq!(asm.outputs.len(), 1);
        assert_eq!(
            asm.inputs,
            [
                super::AsmOperand::Mem,
                super::AsmOperand::MemWrite,
                super::AsmOperand::MemReadWrite
            ]
        );
        assert_eq!(
            asm,
            super::InlineAsm::new("", &asm.constraints(), &[crate::Type::I64]).unwrap()
        );
        for ty in ["ptr", "i64"] {
            let m = crate::parse(&format!(
                "func @f({ty}) {{\nblock0(v0: {ty}):\n asm \"\", \"mreadwrite\"(v0)\n return\n}}\n"
            ))
            .unwrap();
            assert_eq!(crate::verify_module(&m).is_ok(), ty == "ptr");
            let f = &m.functions[0];
            let i = f.layout_insts().next().unwrap().1;
            assert!(f.inst_data(i).is_pinned());
            assert!(f.inst_data(i).memory_effects().may_write());
        }
    }
}
