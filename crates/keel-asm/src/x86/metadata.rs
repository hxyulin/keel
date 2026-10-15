use super::{Assembler, Fix, Sym};
use keel_asm_ir::{
    Expression,
    x86::{self, Address, Modifier, RegisterClass},
};

pub(super) struct Frame {
    section: usize,
    start: u64,
    end: u64,
    last: u64,
    code: Vec<u8>,
    state_depth: usize,
}
pub(super) struct Location {
    pub section: usize,
    pub offset: u64,
    pub file: u32,
    pub line: u64,
    pub column: u64,
    pub prologue_end: bool,
}
pub(super) fn leb(out: &mut Vec<u8>, mut n: i128, signed: bool) -> Result<(), String> {
    if !signed && n < 0 {
        return Err("negative unsigned LEB128".into());
    }
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        let done = if signed {
            (n == 0 && byte & 0x40 == 0) || (n == -1 && byte & 0x40 != 0)
        } else {
            n == 0
        };
        out.push(byte | if done { 0 } else { 0x80 });
        if done {
            break;
        }
    }
    Ok(())
}
fn dwarf_register(s: &str) -> Result<u8, String> {
    if let Some(r) = x86::Register::parse(s) {
        if r.class != RegisterClass::Gpr {
            return Err("CFI requires general registers or DWARF numbers".into());
        }
        return Ok([0, 2, 1, 3, 7, 6, 4, 5, 8, 9, 10, 11, 12, 13, 14, 15][r.number as usize]);
    }
    u8::try_from(x86::integer(s)?).map_err(|_| "invalid DWARF register".into())
}
pub(super) fn cfi(a: &mut Assembler, op: &str, args: &str) -> Result<(), String> {
    let at = a.offset();
    if op == ".cfi_startproc" {
        if !args.is_empty() || a.cfi.is_some() {
            return Err("invalid or nested .cfi_startproc".into());
        }
        a.cfi = Some(Frame {
            section: a.current,
            start: at,
            end: at,
            last: at,
            code: Vec::new(),
            state_depth: 0,
        });
        return Ok(());
    }
    let f = a.cfi.as_mut().ok_or("CFI directive outside procedure")?;
    if f.section != a.current {
        return Err("CFI procedure crosses sections".into());
    }
    if op == ".cfi_endproc" {
        if !args.is_empty() || f.state_depth != 0 {
            return Err("invalid CFI procedure end/state stack".into());
        }
        let mut f = a.cfi.take().unwrap();
        f.end = at;
        a.frames.push(f);
        return Ok(());
    }
    advance(f, at)?;
    let p = x86::split(args, ',');
    match op {
        ".cfi_def_cfa_offset" => {
            if p.len() != 1 {
                return Err("invalid .cfi_def_cfa_offset".into());
            }
            f.code.push(0x0e);
            leb(&mut f.code, x86::integer(args)?, false)?
        }
        ".cfi_def_cfa_register" => {
            if p.len() != 1 {
                return Err("invalid .cfi_def_cfa_register".into());
            }
            f.code.push(0x0d);
            leb(&mut f.code, dwarf_register(args)?.into(), false)?
        }
        ".cfi_def_cfa" if p.len() == 2 => {
            f.code.push(0x0c);
            leb(&mut f.code, dwarf_register(p[0])?.into(), false)?;
            leb(&mut f.code, x86::integer(p[1])?, false)?
        }
        ".cfi_offset" if p.len() == 2 => {
            let reg = dwarf_register(p[0])?;
            let offset = x86::integer(p[1])?;
            if offset > 0 || offset % 8 != 0 {
                return Err("CFI offset must be a nonpositive multiple of eight".into());
            }
            if reg < 64 {
                f.code.push(0x80 | reg)
            } else {
                f.code.push(5);
                leb(&mut f.code, reg.into(), false)?
            }
            leb(&mut f.code, offset / -8, false)?
        }
        ".cfi_restore" if p.len() == 1 => {
            let reg = dwarf_register(args)?;
            if reg < 64 {
                f.code.push(0xc0 | reg)
            } else {
                f.code.push(6);
                leb(&mut f.code, reg.into(), false)?
            }
        }
        ".cfi_remember_state" if args.is_empty() => {
            f.code.push(0x0a);
            f.state_depth += 1
        }
        ".cfi_restore_state" if args.is_empty() && f.state_depth > 0 => {
            f.code.push(0x0b);
            f.state_depth -= 1
        }
        _ => return Err(format!("unsupported CFI directive/operands {op} {args}")),
    }
    Ok(())
}
fn u32_at(bytes: &mut [u8], at: usize, n: usize) -> Result<(), String> {
    bytes[at..at + 4].copy_from_slice(
        &u32::try_from(n)
            .map_err(|_| "DWARF record too large")?
            .to_le_bytes(),
    );
    Ok(())
}
fn symbol(a: &mut Assembler, section: usize, offset: u64) -> String {
    let name = format!(".Lkeel.metadata.{}", a.symbols.len());
    a.symbols.insert(
        name.clone(),
        Sym {
            section: Some(section),
            value: offset,
            ..Sym::default()
        },
    );
    name
}
pub(super) fn finish(a: &mut Assembler) -> Result<(), String> {
    if !a.frames.is_empty() {
        a.section(".eh_frame", Some(2), false)?;
        a.align(3, None)?;
        if a.offset() != 0 {
            return Err("generated CFI conflicts with explicit .eh_frame data".into());
        }
        // CIE: version 1, zR, code alignment 1, data alignment -8,
        // return register RIP, PC-relative signed 32-bit FDE pointers.
        let mut bytes = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 1, b'z', b'R', 0, 1, 0x78, 16, 1, 0x1b, 0x0c, 7, 8, 0x90, 1,
        ];
        while !bytes.len().is_multiple_of(8) {
            bytes.push(0)
        }
        let len = bytes.len() - 4;
        u32_at(&mut bytes, 0, len)?;
        a.bytes(&bytes)?;
        for f in std::mem::take(&mut a.frames) {
            let start = a.offset();
            let name = symbol(a, f.section, f.start);
            let mut b = vec![0; 4];
            b.extend(
                u32::try_from(start + 4)
                    .map_err(|_| "CIE offset too large")?
                    .to_le_bytes(),
            );
            b.extend([0; 4]);
            b.extend(
                u32::try_from(f.end - f.start)
                    .map_err(|_| "CFI function too large")?
                    .to_le_bytes(),
            );
            b.push(0);
            b.extend(f.code);
            while !b.len().is_multiple_of(8) {
                b.push(0)
            }
            let n = b.len() - 4;
            u32_at(&mut b, 0, n)?;
            a.fixes.push(Fix {
                section: a.current,
                offset: start + 8,
                width: 4,
                address: Address {
                    expression: Expression::symbol(name),
                    modifier: Modifier::None,
                },
                kind: 2,
                line: 0,
            });
            a.bytes(&b)?;
        }
    }
    if !a.locations.is_empty() {
        a.section(".debug_line", Some(0), false)?;
        if a.offset() != 0 {
            return Err("generated line table conflicts with explicit .debug_line data".into());
        }
        let mut b = vec![0; 4];
        b.extend(4u16.to_le_bytes());
        b.extend([0; 4]);
        b.extend([1, 1, 1, (-5i8) as u8, 14, 13]);
        b.extend([0, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1]);
        b.push(0);
        let max = *a.files.keys().next_back().ok_or("line table needs .file")?;
        if max > 1_000_000 {
            return Err("file index too large".into());
        }
        for i in 1..=max {
            let file = a
                .files
                .get(&i)
                .ok_or(".file indices must be contiguous from 1")?;
            b.extend(file);
            b.extend([0, 0, 0, 0]);
        }
        b.push(0);
        let hlen = b.len() - 10;
        u32_at(&mut b, 6, hlen)?;
        let locations = std::mem::take(&mut a.locations);
        let (mut current, mut offset, mut line) = (None, 0, 1i128);
        for loc in locations {
            if current != Some(loc.section) {
                if current.is_some() {
                    b.extend([0, 1, 1]);
                } // DW_LNE_end_sequence
                let name = symbol(a, loc.section, loc.offset);
                b.extend([0, 9, 2]);
                let at = b.len();
                b.extend([0; 8]);
                a.fixes.push(Fix {
                    section: a.current,
                    offset: at as u64,
                    width: 8,
                    address: Address {
                        expression: Expression::symbol(name),
                        modifier: Modifier::None,
                    },
                    kind: 1,
                    line: 0,
                });
                current = Some(loc.section);
                offset = loc.offset;
                line = 1;
            }
            if loc.offset < offset {
                return Err("line addresses are not monotonic".into());
            }
            b.push(2);
            leb(&mut b, (loc.offset - offset).into(), false)?;
            offset = loc.offset;
            b.push(4);
            leb(&mut b, loc.file.into(), false)?;
            b.push(5);
            leb(&mut b, loc.column.into(), false)?;
            b.push(3);
            leb(&mut b, i128::from(loc.line) - line, true)?;
            line = loc.line.into();
            if loc.prologue_end {
                b.push(10)
            }
            b.push(1);
        }
        if let Some(sec) = current {
            let end = a.sections[sec].size;
            b.push(2);
            leb(&mut b, (end - offset).into(), false)?;
            b.extend([0, 1, 1]);
        }
        let len = b.len() - 4;
        u32_at(&mut b, 0, len)?;
        a.bytes(&b)?;
    }
    Ok(())
}

fn advance(f: &mut Frame, at: u64) -> Result<(), String> {
    let advance = at - f.last;
    if advance > 0 {
        if advance < 64 {
            f.code.push(0x40 | advance as u8)
        } else if advance <= 255 {
            f.code.extend([2, advance as u8])
        } else if advance <= 65535 {
            f.code.push(3);
            f.code.extend((advance as u16).to_le_bytes())
        } else {
            f.code.push(4);
            f.code.extend(
                u32::try_from(advance)
                    .map_err(|_| "CFI advance exceeds 32 bits")?
                    .to_le_bytes(),
            )
        }
        f.last = at
    }
    Ok(())
}
pub(super) fn shared_cfi(a: &mut Assembler, c: &keel_asm_ir::Cfi) -> Result<(), String> {
    use keel_asm_ir::Cfi;
    if matches!(c, Cfi::Start) {
        return cfi(a, ".cfi_startproc", "");
    }
    if matches!(c, Cfi::End) {
        return cfi(a, ".cfi_endproc", "");
    }
    let at = a.offset();
    let f = a.cfi.as_mut().ok_or("CFI directive outside procedure")?;
    if f.section != a.current {
        return Err("CFI procedure crosses sections".into());
    }
    advance(f, at)?;
    match c {
        Cfi::DefCfaOffset(n) => {
            f.code.push(0x0e);
            leb(&mut f.code, (*n).into(), false)
        }
        Cfi::DefCfaRegister(r) => {
            f.code.push(0x0d);
            leb(&mut f.code, (*r).into(), false)
        }
        Cfi::DefCfa { register, offset } => {
            f.code.push(0x0c);
            leb(&mut f.code, (*register).into(), false)?;
            leb(&mut f.code, (*offset).into(), false)
        }
        Cfi::Offset { register, offset } => {
            if *offset > 0 || offset % 8 != 0 {
                return Err("CFI offset must be a nonpositive multiple of eight".into());
            }
            if *register < 64 {
                f.code.push(0x80 | *register);
            } else {
                f.code.push(5);
                leb(&mut f.code, (*register).into(), false)?;
            }
            leb(&mut f.code, i128::from(*offset) / -8, false)
        }
        _ => Err("x86 exception personality/LSDA directives are unsupported".into()),
    }
}
