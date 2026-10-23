//! Architectural x87 forms beyond the original compiler-output subset.
use super::*;
use keel_asm_ir::x86::X87Opcode as X;

fn st(operand: &Operand) -> Option<u8> {
    match operand {
        R(r) if r.class == RegisterClass::X87 => Some(r.number),
        _ => None,
    }
}

pub(super) fn encode<'a>(e: &mut Encoding<'a>, i: &'a Instruction, op: X) -> Result<(), String> {
    let args = i.operands.as_slice();
    let invalid = || format!("unsupported x87 operands for {}: {args:?}", op.mnemonic());
    if let [m @ M(memory)] = args {
        let w = memory.width;
        let arithmetic = match op {
            X::Fsub => Some(4),
            X::Fsubr => Some(5),
            X::Fdiv => Some(6),
            X::Fdivr => Some(7),
            X::Fcom => Some(2),
            X::Fcomp => Some(3),
            _ => None,
        };
        let integer = match op {
            X::Fiadd => Some(0),
            X::Fimul => Some(1),
            X::Ficom => Some(2),
            X::Ficomp => Some(3),
            X::Fisub => Some(4),
            X::Fisubr => Some(5),
            X::Fidiv => Some(6),
            X::Fidivr => Some(7),
            _ => None,
        };
        let (wait, prefix, byte, group) = if let Some(group) = arithmetic {
            (
                false,
                0,
                match w {
                    32 => 0xd8,
                    64 => 0xdc,
                    _ => return Err(invalid()),
                },
                group,
            )
        } else if let Some(group) = integer {
            (
                false,
                0,
                match w {
                    16 => 0xde,
                    32 => 0xda,
                    _ => return Err(invalid()),
                },
                group,
            )
        } else {
            match op {
                X::Fst if w == 32 => (false, 0, 0xd9, 2),
                X::Fst if w == 64 => (false, 0, 0xdd, 2),
                X::Fist if w == 16 => (false, 0, 0xdf, 2),
                X::Fist if w == 32 => (false, 0, 0xdb, 2),
                X::Fbld | X::Fbstp if matches!(w, 0 | 80) => {
                    (false, 0, 0xdf, if op == X::Fbld { 4 } else { 6 })
                }
                X::Fstcw if matches!(w, 0 | 16) => (true, 0, 0xd9, 7),
                X::Fstsw | X::Fnstsw if matches!(w, 0 | 16) => (op == X::Fstsw, 0, 0xdd, 7),
                X::Fldenv | X::Fstenv | X::Fnstenv | X::Fsave | X::Fnsave | X::Frstor
                    if matches!(w, 0 | 16 | 32) =>
                {
                    // Typed width selects the 16/32-bit environment layout,
                    // not a two/four-byte memory access. Text uses DATA16.
                    let (byte, group) = match op {
                        X::Fldenv => (0xd9, 4),
                        X::Fstenv | X::Fnstenv => (0xd9, 6),
                        X::Frstor => (0xdd, 4),
                        _ => (0xdd, 6),
                    };
                    (
                        matches!(op, X::Fstenv | X::Fsave),
                        if w == 16 { 0x66 } else { 0 },
                        byte,
                        group,
                    )
                }
                _ => return Err(invalid()),
            }
        };
        // FWAIT is a separate instruction. The environment-size override
        // must prefix the following x87 instruction, never FWAIT itself.
        if wait {
            e.bytes.push(0x9b);
        }
        return e.rm(prefix, &[byte], 32, group, m, None);
    }
    if matches!(op, X::Fnstsw | X::Fstsw)
        && matches!(args, [R(r)] if r.class == RegisterClass::Gpr && r.number == 0 && r.width == 16)
    {
        if op == X::Fstsw {
            e.bytes.push(0x9b);
        }
        e.bytes.extend_from_slice(&[0xdf, 0xe0]);
        return Ok(());
    }
    let (byte, low) = match op {
        X::Fst | X::Ffree | X::Ffreep => {
            let [arg] = args else { return Err(invalid()) };
            let index = st(arg).ok_or_else(invalid)?;
            (
                if op == X::Ffreep { 0xdf } else { 0xdd },
                if op == X::Fst { 0xd0 } else { 0xc0 } + index,
            )
        }
        X::Fsub | X::Fsubr | X::Fdiv | X::Fdivr => {
            let (byte, index) = match args {
                [arg] if st(arg).is_some() => (0xd8, st(arg).unwrap()),
                [a, b] if st(a) == Some(0) && st(b).is_some() => (0xd8, st(b).unwrap()),
                [a, b] if st(a).is_some() && st(b) == Some(0) => (0xdc, st(a).unwrap()),
                _ => return Err(invalid()),
            };
            let base = match (op, byte) {
                (X::Fsub, 0xd8) | (X::Fsubr, 0xdc) => 0xe0,
                (X::Fsubr, 0xd8) | (X::Fsub, 0xdc) => 0xe8,
                (X::Fdiv, 0xd8) | (X::Fdivr, 0xdc) => 0xf0,
                _ => 0xf8,
            };
            (byte, base + index)
        }
        X::Faddp | X::Fdivp | X::Fdivrp => {
            let index = match args {
                [] => 1,
                [a] if st(a).is_some() => st(a).unwrap(),
                [a, b] if st(a).is_some() && st(b) == Some(0) => st(a).unwrap(),
                _ => return Err(invalid()),
            };
            (
                0xde,
                match op {
                    X::Faddp => 0xc0,
                    X::Fdivp => 0xf8,
                    _ => 0xf0,
                } + index,
            )
        }
        X::Fcom | X::Fcomp | X::Fucom | X::Fucomp | X::Fcomi | X::Fcomip => {
            let index = match args {
                [] => 1,
                [a] if st(a).is_some() => st(a).unwrap(),
                [a, b]
                    if matches!(op, X::Fcomi | X::Fcomip)
                        && st(a) == Some(0)
                        && st(b).is_some() =>
                {
                    st(b).unwrap()
                }
                _ => return Err(invalid()),
            };
            match op {
                X::Fcom => (0xd8, 0xd0 + index),
                X::Fcomp => (0xd8, 0xd8 + index),
                X::Fucom => (0xdd, 0xe0 + index),
                X::Fucomp => (0xdd, 0xe8 + index),
                X::Fcomi => (0xdb, 0xf0 + index),
                _ => (0xdf, 0xf0 + index),
            }
        }
        X::Fcmovb | X::Fcmove | X::Fcmovbe | X::Fcmovu | X::Fcmovne | X::Fcmovnbe | X::Fcmovnu => {
            let [a, b] = args else { return Err(invalid()) };
            if st(a) != Some(0) {
                return Err(invalid());
            }
            let index = st(b).ok_or_else(invalid)?;
            let (byte, base) = match op {
                X::Fcmovb => (0xda, 0xc0),
                X::Fcmove => (0xda, 0xc8),
                X::Fcmovbe => (0xda, 0xd0),
                X::Fcmovu => (0xda, 0xd8),
                X::Fcmovne => (0xdb, 0xc8),
                X::Fcmovnbe => (0xdb, 0xd0),
                _ => (0xdb, 0xd8),
            };
            (byte, base + index)
        }
        _ => return Err(invalid()),
    };
    e.bytes.extend_from_slice(&[byte, low]);
    Ok(())
}
