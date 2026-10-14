//! Inline assembly: a template of target instructions with `%0`, `%1`…
//! standing for operands, and a constraint string, in the style GCC and
//! LLVM share, saying what each operand is:
//!
//! ```text
//! v2 = asm "add %0, %1, %2", "=r,r,r"(v0, v1) -> i64
//! v3 = asm "ldr %0, %1", "=r,m,~{memory}"(v0) -> i64
//! ```
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
    /// `i` or `n`: a constant, the operand being an `iconst`. Inputs only.
    Imm,
    /// `0`, `1`…: in the same register as that output, which it initializes.
    /// Inputs only.
    Tied(u16),
}

/// A register by class and number: `x5` and `w5` are the general-purpose
/// register 5, `v5`, `d5` and `s5` the SIMD/FP register 5.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PhysReg {
    pub float: bool,
    pub num: u8,
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
                // `&` (early clobber) asks for a register no input uses,
                // which every output gets anyway.
                let code = code.strip_prefix('&').unwrap_or(code);
                let operand = operand(code)?;
                if !matches!(
                    operand,
                    AsmOperand::Reg | AsmOperand::FloatReg | AsmOperand::Fixed(_)
                ) {
                    return Err(format!("output `{part}` must be a register"));
                }
                output_kinds.push(operand);
            } else {
                asm.inputs.push(operand(part)?);
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
        Ok(asm)
    }

    /// The constraint string, as [`InlineAsm::new`] reads it.
    pub fn constraints(&self) -> String {
        let outputs = self.outputs.iter().map(|(op, _)| format!("={op}"));
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
        "i" | "n" => AsmOperand::Imm,
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
    /// their `q`, `d`, `s`, `h` and `b` views.
    pub fn parse(name: &str) -> Result<Self, String> {
        let bad = || format!("unknown register `{name}`");
        match name {
            "fp" => {
                return Ok(PhysReg {
                    float: false,
                    num: 29,
                });
            }
            "lr" => {
                return Ok(PhysReg {
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
        Ok(PhysReg { float, num })
    }
}

impl fmt::Display for PhysReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            AsmOperand::Imm => f.write_str("i"),
            AsmOperand::Tied(n) => write!(f, "{n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "=r,={x3},r,{v1},m,i,0,~{x30},~{memory},~{cc}"
        );
        let again = InlineAsm::new("", &asm.constraints(), &[Type::I64, Type::I32]).unwrap();
        assert_eq!(asm, again);
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
        assert!(err("~{sp}", 0).contains("unknown register"));
    }
}
