//! Typed ordinary application core encodings; fixed rel8 branches never widen.
use super::*;
use keel_asm_ir::x86::{CoreOpcode, LoopKind, Repeat, StringKind};
fn unsigned(o: &Operand, bits: u32) -> Result<i128, String> {
    constant(immediate(o)?)
        .filter(|n| *n >= 0 && *n < (1i128 << bits))
        .ok_or_else(|| format!("core operand requires a constant unsigned {bits}-bit immediate"))
}
pub(super) fn encode<'a>(
    e: &mut Encoding<'a>,
    i: &'a Instruction,
    op: CoreOpcode,
) -> Result<(), String> {
    let o = i.operands.as_slice();
    if o.iter()
        .any(|o| matches!(o,M(m) if m.displacement.modifier==Modifier::GotTpOff))
    {
        return Err("GOTTPOFF relaxation is not valid on these core operands".into());
    }
    let bad = || format!("invalid operands for {}", op.mnemonic());
    // Explicit historical hint-space NOP encodings. They are inventoried as
    // encodings only; callers must not assume their availability on every CPU.
    let hint = match op {
        CoreOpcode::Nop18_0 => Some((24, 0)),
        CoreOpcode::Nop18_1 => Some((24, 1)),
        CoreOpcode::Nop18_2 => Some((24, 2)),
        CoreOpcode::Nop18_3 => Some((24, 3)),
        CoreOpcode::Nop18_4 => Some((24, 4)),
        CoreOpcode::Nop18_5 => Some((24, 5)),
        CoreOpcode::Nop18_6 => Some((24, 6)),
        CoreOpcode::Nop18_7 => Some((24, 7)),
        CoreOpcode::Nop19 => Some((25, 0)),
        CoreOpcode::Nop1a => Some((26, 0)),
        CoreOpcode::Nop1b => Some((27, 0)),
        CoreOpcode::Nop1c => Some((28, 0)),
        CoreOpcode::Nop1d => Some((29, 0)),
        CoreOpcode::Nop1e => Some((30, 0)),
        _ => None,
    };
    if let Some((byte, group)) = hint {
        let (rm, field) = match (byte, o) {
            (0x18, [rm]) if !matches!(rm, M(_)) || group >= 4 => (rm, group),
            (0x19..=0x1e, [rm, R(r)])
                if r.class == RegisterClass::Gpr && matches!(r.width, 16 | 32 | 64) =>
            {
                check_width(rm, r.width)?;
                (rm, r.number)
            }
            _ => return Err(bad()),
        };
        let bits = width(rm);
        if !matches!(bits, 16 | 32 | 64) {
            return Err(bad());
        }
        check_width(rm, bits)?;
        return e.rm(0, &[0x0f, byte], bits, field, rm, None);
    }
    match (op, o) {
        (
            CoreOpcode::String {
                kind,
                width,
                repeat,
                address32,
                segment,
            },
            [],
        ) => {
            if !matches!(width, 8 | 16 | 32 | 64)
                || matches!(kind, StringKind::Ins | StringKind::Outs) && width == 64
                || segment.is_some_and(|s| !matches!(s, 0x64 | 0x65))
                || segment.is_some()
                    && !matches!(
                        kind,
                        StringKind::Movs | StringKind::Lods | StringKind::Cmps | StringKind::Outs
                    )
            {
                return Err(bad());
            }
            if let Some(s) = segment {
                e.bytes.push(s);
            }
            if address32 {
                e.bytes.push(0x67);
            }
            if width == 16 {
                e.bytes.push(0x66);
            }
            match repeat {
                Repeat::None => {}
                Repeat::Rep => e.bytes.push(0xf3),
                Repeat::Repne => e.bytes.push(0xf2),
            }
            if width == 64 {
                e.bytes.push(0x48);
            }
            e.bytes.push(
                match kind {
                    StringKind::Movs => 0xa4,
                    StringKind::Cmps => 0xa6,
                    StringKind::Stos => 0xaa,
                    StringKind::Lods => 0xac,
                    StringKind::Scas => 0xae,
                    StringKind::Ins => 0x6c,
                    StringKind::Outs => 0x6e,
                } + u8::from(width != 8),
            );
        }
        (CoreOpcode::MovMoffs32, [a, b]) => {
            let (r, m, store) = match (a, b) {
                (R(r), M(m)) => (r, m, false),
                (M(m), R(r)) => (r, m, true),
                _ => return Err(bad()),
            };
            if r.class != RegisterClass::Gpr
                || r.number != 0
                || r.high_byte
                || !matches!(r.width, 8 | 16 | 32 | 64)
                || m.base.is_some()
                || m.index.is_some()
                || (m.width != 0 && m.width != r.width)
            {
                return Err(bad());
            }
            if let Some(segment) = m.segment {
                e.bytes.push(segment);
            }
            e.bytes.push(0x67);
            if r.width == 16 {
                e.bytes.push(0x66);
            }
            if r.width == 64 {
                e.bytes.push(0x48);
            }
            e.bytes
                .push(0xa0 + u8::from(r.width != 8) + 2 * u8::from(store));
            e.imm(&m.displacement, 4, false)?;
        }
        (
            CoreOpcode::MovLoad
            | CoreOpcode::AddLoad
            | CoreOpcode::OrLoad
            | CoreOpcode::AdcLoad
            | CoreOpcode::SbbLoad
            | CoreOpcode::AndLoad
            | CoreOpcode::SubLoad
            | CoreOpcode::XorLoad
            | CoreOpcode::CmpLoad,
            [R(dst), src],
        ) if dst.class == RegisterClass::Gpr && matches!(dst.width, 8 | 16 | 32 | 64) => {
            check_width(src, dst.width)?;
            let byte = match op {
                CoreOpcode::MovLoad => 0x8a,
                CoreOpcode::AddLoad => 0x02,
                CoreOpcode::OrLoad => 0x0a,
                CoreOpcode::AdcLoad => 0x12,
                CoreOpcode::SbbLoad => 0x1a,
                CoreOpcode::AndLoad => 0x22,
                CoreOpcode::SubLoad => 0x2a,
                CoreOpcode::XorLoad => 0x32,
                CoreOpcode::CmpLoad => 0x3a,
                _ => unreachable!(),
            };
            e.rm(
                0,
                &[byte + u8::from(dst.width != 8)],
                dst.width,
                dst.number,
                src,
                Some(*dst),
            )?;
        }
        (CoreOpcode::MovImm, [dst, I(imm)]) => {
            let bits = width(dst);
            if !matches!(bits, 8 | 16 | 32 | 64) {
                return Err(bad());
            }
            check_width(dst, bits)?;
            e.rm(
                0,
                &[if bits == 8 { 0xc6 } else { 0xc7 }],
                bits,
                0,
                dst,
                None,
            )?;
            e.imm(
                imm,
                if bits == 64 { 4 } else { (bits / 8) as u8 },
                bits == 64,
            )?;
        }
        (CoreOpcode::PushRm | CoreOpcode::PopRm, [dst]) => {
            let bits = width(dst);
            if !matches!(bits, 16 | 64) {
                return Err(bad());
            }
            check_width(dst, bits)?;
            e.rm(
                0,
                &[if op == CoreOpcode::PushRm { 0xff } else { 0x8f }],
                if bits == 16 { 16 } else { 32 },
                if op == CoreOpcode::PushRm { 6 } else { 0 },
                dst,
                None,
            )?;
        }
        (CoreOpcode::Pushw | CoreOpcode::Popw, [R(r)])
            if r.class == RegisterClass::Gpr && r.width == 16 =>
        {
            e.bytes.push(0x66);
            if r.number >= 8 {
                e.bytes.push(0x41);
            }
            e.bytes
                .push((if op == CoreOpcode::Pushw { 0x50 } else { 0x58 }) + (r.number & 7));
        }
        (CoreOpcode::Pushw | CoreOpcode::Popw, [a @ M(m)]) if m.width == 0 || m.width == 16 => {
            e.rm(
                0,
                &[if op == CoreOpcode::Pushw { 0xff } else { 0x8f }],
                16,
                if op == CoreOpcode::Pushw { 6 } else { 0 },
                a,
                None,
            )?;
        }
        (CoreOpcode::Pushw | CoreOpcode::Popw, [R(r)])
            if r.class == RegisterClass::Segment && matches!(r.number, 4 | 5) =>
        {
            e.bytes.extend([
                0x66,
                0x0f,
                0xa0 + 8 * (r.number - 4) + u8::from(op == CoreOpcode::Popw),
            ]);
        }
        (CoreOpcode::Pushw, [I(imm)]) => {
            let small = constant(imm).is_some_and(|n| (-128..128).contains(&n));
            e.bytes.extend([0x66, if small { 0x6a } else { 0x68 }]);
            e.imm(imm, if small { 1 } else { 2 }, true)?;
        }
        (CoreOpcode::XchgAcc, [R(a), R(b)])
            if a.class == RegisterClass::Gpr
                && b.class == RegisterClass::Gpr
                && a.width == b.width
                && matches!(a.width, 16 | 32 | 64)
                && (a.number == 0 || b.number == 0) =>
        {
            let r = if a.number == 0 { b } else { a };
            // 90 with no REX.B is NOP and cannot implement EAX's zero extension.
            if r.number == 0 {
                return Err(bad());
            }
            if r.width == 16 {
                e.bytes.push(0x66);
            }
            let rex = 0x40 | if r.width == 64 { 8 } else { 0 } | (r.number >> 3);
            if rex != 0x40 {
                e.bytes.push(rex);
            }
            e.bytes.push(0x90 + (r.number & 7));
        }
        (CoreOpcode::Xlat { address32, segment }, []) => {
            if segment.is_some_and(|s| !matches!(s, 0x64 | 0x65)) {
                return Err(bad());
            }
            if let Some(s) = segment {
                e.bytes.push(s);
            }
            if address32 {
                e.bytes.push(0x67);
            }
            e.bytes.push(0xd7);
        }
        (CoreOpcode::Rcl | CoreOpcode::Rcr | CoreOpcode::RclImm | CoreOpcode::RcrImm, [a, b]) => {
            let bits = width(a);
            if !matches!(bits, 8 | 16 | 32 | 64) {
                return Err(bad());
            }
            check_width(a, bits)?;
            let cl = matches!(b,R(r) if r.class==RegisterClass::Gpr && r.number==1 && r.width==8 && !r.high_byte);
            let force = matches!(op, CoreOpcode::RclImm | CoreOpcode::RcrImm);
            if force && cl {
                return Err(bad());
            }
            let n = if cl { 0 } else { unsigned(b, 8)? };
            let one = n == 1 && !force;
            let byte = if cl {
                0xd2
            } else if one {
                0xd0
            } else {
                0xc0
            } + u8::from(bits != 8);
            e.rm(
                0,
                &[byte],
                bits,
                if matches!(op, CoreOpcode::Rcl | CoreOpcode::RclImm) {
                    2
                } else {
                    3
                },
                a,
                None,
            )?;
            if !cl && !one {
                e.number(n, 1);
            }
        }
        (CoreOpcode::Cwd, []) => e.bytes.extend_from_slice(&[0x66, 0x99]),
        (CoreOpcode::Cmpxchg8b, [a @ M(m)]) if matches!(m.width, 0 | 64) => {
            e.rm(0, &[0x0f, 0xc7], 32, 1, a, None)?
        }
        (CoreOpcode::Clflush, [a @ M(m)]) if matches!(m.width, 0 | 8) => {
            e.rm(0, &[0x0f, 0xae], 32, 7, a, None)?
        }
        (CoreOpcode::Loop { kind, address32 }, [I(a)]) => {
            if a.modifier != Modifier::None
                || a.expression.add.is_none()
                || a.expression.subtract.is_some()
            {
                return Err("short core branch requires a symbolic target without a relocation modifier or symbol difference".into());
            }
            if address32 {
                e.bytes.push(0x67);
            }
            e.bytes.push(match kind {
                LoopKind::Loop => 0xe2,
                LoopKind::Equal => 0xe1,
                LoopKind::NotEqual => 0xe0,
                LoopKind::Zero => 0xe3,
            });
            e.branch(a, false, 1)?;
        }
        (CoreOpcode::Enter { operand16 }, [a, b]) => {
            let frame = unsigned(a, 16)?;
            let nesting = unsigned(b, 8)?;
            if operand16 {
                e.bytes.push(0x66);
            }
            e.bytes.push(0xc8);
            e.number(frame, 2);
            e.number(nesting, 1);
        }
        (CoreOpcode::Int | CoreOpcode::IntImm, [a]) => {
            let vector = unsigned(a, 8)?;
            if vector == 3 && op == CoreOpcode::Int {
                e.bytes.push(0xcc);
            } else {
                e.bytes.push(0xcd);
                e.number(vector, 1);
            }
        }
        (CoreOpcode::Int1, []) => e.bytes.push(0xf1),
        (CoreOpcode::In, [R(acc), port])
            if acc.class == RegisterClass::Gpr
                && acc.number == 0
                && !acc.high_byte
                && matches!(acc.width, 8 | 16 | 32) =>
        {
            let immediate_port = match port {
                R(r) if r.class == RegisterClass::Gpr && r.number == 2 && r.width == 16 => None,
                I(_) => Some(unsigned(port, 8)?),
                _ => return Err(bad()),
            };
            if acc.width == 16 {
                e.bytes.push(0x66);
            }
            e.bytes
                .push(match (acc.width == 8, immediate_port.is_some()) {
                    (true, true) => 0xe4,
                    (false, true) => 0xe5,
                    (true, false) => 0xec,
                    (false, false) => 0xed,
                });
            if let Some(n) = immediate_port {
                e.number(n, 1);
            }
        }
        (CoreOpcode::Out, [port, R(acc)])
            if acc.class == RegisterClass::Gpr
                && acc.number == 0
                && !acc.high_byte
                && matches!(acc.width, 8 | 16 | 32) =>
        {
            let immediate_port = match port {
                R(r) if r.class == RegisterClass::Gpr && r.number == 2 && r.width == 16 => None,
                I(_) => Some(unsigned(port, 8)?),
                _ => return Err(bad()),
            };
            if acc.width == 16 {
                e.bytes.push(0x66);
            }
            e.bytes
                .push(match (acc.width == 8, immediate_port.is_some()) {
                    (true, true) => 0xe6,
                    (false, true) => 0xe7,
                    (true, false) => 0xee,
                    (false, false) => 0xef,
                });
            if let Some(n) = immediate_port {
                e.number(n, 1);
            }
        }
        (CoreOpcode::Lfs | CoreOpcode::Lgs | CoreOpcode::Lss, [R(dst), a @ M(m)])
            if dst.class == RegisterClass::Gpr
                && matches!(dst.width, 16 | 32 | 64)
                && matches!(m.width, 0 | 32 | 48 | 80)
                && (m.width == 0 || m.width == dst.width + 16) =>
        {
            let byte = match op {
                CoreOpcode::Lfs => 0xb4,
                CoreOpcode::Lgs => 0xb5,
                CoreOpcode::Lss => 0xb2,
                _ => unreachable!(),
            };
            e.rm(0, &[0x0f, byte], dst.width, dst.number, a, Some(*dst))?;
        }
        (CoreOpcode::Lar | CoreOpcode::Lsl, [R(dst), src])
            if dst.class == RegisterClass::Gpr && matches!(dst.width, 16 | 32 | 64) =>
        {
            match src {
                // Assemblers also accept a 16-bit selector spelling. XED uses
                // GPRv for LAR and GPRz (32 bits with REX.W) for LSL.
                R(r) if r.class == RegisterClass::Gpr
                    && (r.width == 16
                        || r.width == dst.width
                        || (op == CoreOpcode::Lsl && dst.width == 64 && r.width == 32)) => {}
                M(m) if matches!(m.width, 0 | 16) => {}
                _ => return Err(bad()),
            }
            e.rm(
                0,
                &[0x0f, if op == CoreOpcode::Lar { 0x02 } else { 0x03 }],
                dst.width,
                dst.number,
                src,
                Some(*dst),
            )?;
        }
        (CoreOpcode::Verr | CoreOpcode::Verw, [src]) => {
            check_width(src, 16)?;
            e.rm(
                0,
                &[0x0f, 0x00],
                32,
                if op == CoreOpcode::Verr { 4 } else { 5 },
                src,
                None,
            )?;
        }
        (CoreOpcode::Sgdt | CoreOpcode::Sidt | CoreOpcode::Lgdt | CoreOpcode::Lidt, [a @ M(m)])
            if matches!(m.width, 0 | 80) =>
        {
            e.rm(
                0,
                &[0x0f, 0x01],
                32,
                match op {
                    CoreOpcode::Sgdt => 0,
                    CoreOpcode::Sidt => 1,
                    CoreOpcode::Lgdt => 2,
                    CoreOpcode::Lidt => 3,
                    _ => unreachable!(),
                },
                a,
                None,
            )?;
        }
        (CoreOpcode::Sldt | CoreOpcode::Str | CoreOpcode::Smsw, [a]) => {
            let bits = match a {
                R(r) if r.class == RegisterClass::Gpr && matches!(r.width, 16 | 32 | 64) => r.width,
                M(m) if matches!(m.width, 0 | 16) => 32,
                _ => return Err(bad()),
            };
            let (opcode, field) = match op {
                CoreOpcode::Sldt => (0x00, 0),
                CoreOpcode::Str => (0x00, 1),
                CoreOpcode::Smsw => (0x01, 4),
                _ => unreachable!(),
            };
            e.rm(0, &[0x0f, opcode], bits, field, a, None)?;
        }
        (CoreOpcode::CallFar | CoreOpcode::JmpFar, [a @ M(m)]) => {
            let bits = match m.width {
                32 => 16,
                48 => 32,
                80 => 64,
                _ => return Err("far pointer memory needs explicit dword/fword/tbyte width".into()),
            };
            e.rm(
                0,
                &[0xff],
                bits,
                if op == CoreOpcode::CallFar { 3 } else { 5 },
                a,
                None,
            )?;
        }
        (CoreOpcode::RetFar { width }, []) if matches!(width, 16 | 32 | 64) => {
            if width == 16 {
                e.bytes.push(0x66);
            }
            if width == 64 {
                e.bytes.push(0x48);
            }
            e.bytes.push(0xcb);
        }
        (CoreOpcode::RetFar { width }, [imm]) if matches!(width, 16 | 32 | 64) => {
            let value = unsigned(imm, 16)?;
            if width == 16 {
                e.bytes.push(0x66);
            }
            if width == 64 {
                e.bytes.push(0x48);
            }
            e.bytes.push(0xca);
            e.number(value, 2);
        }
        (CoreOpcode::Ud0Short, []) => e.bytes.extend_from_slice(&[0x0f, 0xff]),
        (CoreOpcode::Ud0 | CoreOpcode::Ud1, [R(reg), rm])
            if reg.class == RegisterClass::Gpr && reg.width == 32 =>
        {
            check_width(rm, 32)?;
            e.rm(
                0,
                &[0x0f, if op == CoreOpcode::Ud0 { 0xff } else { 0xb9 }],
                32,
                reg.number,
                rm,
                Some(*reg),
            )?;
        }
        _ => return Err(bad()),
    }
    Ok(())
}
