//! x87 forms evidenced in compiler output. The shared operand order and opcode
//! identities use Intel semantics; AT&T subtraction-pop spelling is normalized
//! by its parser. No unsized floating/integer memory operand is inferred here.
use super::*;

fn st(o: &Operand) -> Option<u8> {
    match o {
        R(r) if r.class == RegisterClass::X87 => Some(r.number),
        _ => None,
    }
}

pub(super) fn encode<'a>(e: &mut Encoding<'a>, i: &'a Instruction) -> Result<(), String> {
    let op = i.opcode;
    let o = i.operands.as_slice();
    let invalid = || format!("unsupported operands for {op}: {o:?}");
    if let [m @ M(memory)] = o {
        let (byte, group) = match (op, memory.width) {
            (Fld, 32) => (0xd9, 0),
            (Fld, 64) => (0xdd, 0),
            (Fld, 80) => (0xdb, 5),
            (Fstp, 32) => (0xd9, 3),
            (Fstp, 64) => (0xdd, 3),
            (Fstp, 80) => (0xdb, 7),
            (Fadd, 32) => (0xd8, 0),
            (Fadd, 64) => (0xdc, 0),
            (Fmul, 32) => (0xd8, 1),
            (Fmul, 64) => (0xdc, 1),
            (Fild, 16) => (0xdf, 0),
            (Fild, 32) => (0xdb, 0),
            (Fild, 64) => (0xdf, 5),
            (Fistp, 16) => (0xdf, 3),
            (Fistp, 32) => (0xdb, 3),
            (Fistp, 64) => (0xdf, 7),
            (Fldcw, 16) => (0xd9, 5),
            (Fnstcw, 16) => (0xd9, 7),
            _ => return Err(invalid()),
        };
        // x87 data widths select opcodes, never REX.W or operand-size prefixes.
        // The common emitter still supplies segment, address REX and fixups.
        return e.rm(0, &[byte], 32, group, m, None);
    }
    let (byte, low) = match (op, o) {
        (Fld, [a]) if st(a).is_some() => (0xd9, 0xc0 + st(a).unwrap()),
        (Fstp, [a]) if st(a).is_some() => (0xdd, 0xd8 + st(a).unwrap()),
        (Fxch, []) => (0xd9, 0xc9),
        (Fxch, [a]) if st(a).is_some() => (0xd9, 0xc8 + st(a).unwrap()),
        (Fucomi | Fucomip, _) => {
            let index = match o {
                [] => 1,
                [a] if st(a).is_some() => st(a).unwrap(),
                [a, b] if st(a) == Some(0) && st(b).is_some() => st(b).unwrap(),
                _ => return Err(invalid()),
            };
            (if op == Fucomi { 0xdb } else { 0xdf }, 0xe8 + index)
        }
        (Fadd | Fmul, _) => {
            let (byte, index) = match o {
                [a] if st(a).is_some() => (0xd8, st(a).unwrap()),
                [a, b] if st(a) == Some(0) && st(b).is_some() => (0xd8, st(b).unwrap()),
                [a, b] if st(a).is_some() && st(b) == Some(0) => (0xdc, st(a).unwrap()),
                _ => return Err(invalid()),
            };
            (byte, if op == Fadd { 0xc0 } else { 0xc8 } + index)
        }
        (Fmulp | Fsubp | Fsubrp, _) => {
            let index = match o {
                [] => 1,
                [a] if st(a).is_some() => st(a).unwrap(),
                [a, b] if st(a).is_some() && st(b) == Some(0) => st(a).unwrap(),
                _ => return Err(invalid()),
            };
            (
                0xde,
                match op {
                    Fmulp => 0xc8,
                    Fsubp => 0xe8,
                    _ => 0xe0,
                } + index,
            )
        }
        (Fcmovnb, [a, b]) if st(a) == Some(0) && st(b).is_some() => (0xdb, 0xc0 + st(b).unwrap()),
        (Fldz, []) => (0xd9, 0xee),
        _ => return Err(invalid()),
    };
    e.bytes.extend([byte, low]);
    Ok(())
}
