//! Compile-speed lowering for bounded scalar leaves. Every value has an
//! eight-byte stack home. x9/x10 are scratches; no liveness or allocation runs.
//! Eligibility is transactional: unsupported input leaves the output untouched
//! and uses the ordinary backend. All offsets are local to the encoded body.
use super::{Cfi, Directive, Expression, Function, Output, Sink, Type};
use keel_ir::{BinaryOp, BlockCall, EntityRef, InstData, IntCC, Purpose, Value};

fn word(out: &mut Vec<u8>, bits: u32) {
    out.extend_from_slice(&bits.to_le_bytes());
}
fn patch(out: &mut [u8], position: usize, bits: u32) {
    out[position..position + 4].copy_from_slice(&bits.to_le_bytes());
}
fn home(func: &Function, value: Value) -> u32 {
    func.resolve(value).index() as u32
}
fn load(out: &mut Vec<u8>, register: u32, slot: u32) {
    word(out, 0xf94003e0 | slot << 10 | register);
}
fn store(out: &mut Vec<u8>, register: u32, slot: u32) {
    word(out, 0xf90003e0 | slot << 10 | register);
}
fn edge(func: &Function, dest: &BlockCall, out: &mut Vec<u8>, branches: &mut Vec<(usize, usize)>) {
    // Stage every source before assigning any block parameter, including cycles.
    let temporary = func.num_values() as u32;
    for (index, &value) in dest.args.iter().enumerate() {
        load(out, 9, home(func, value));
        store(out, 9, temporary + index as u32);
    }
    for (index, &value) in func.block_params(dest.block).iter().enumerate() {
        load(out, 9, temporary + index as u32);
        store(out, 9, home(func, value));
    }
    branches.push((out.len(), dest.block.index()));
    word(out, 0x14000000);
}
struct Code {
    bytes: Vec<u8>,
    frame: u32,
    /// Offsets of the ADD sp before each RET, for precise unwind transitions.
    epilogues: Vec<usize>,
}
fn encode(func: &Function) -> Option<Code> {
    if func.sig.variadic
        || func.sig.returns_twice
        || func.personality.is_some()
        || func.sig.params.len() > 8
        || func.sig.returns.len() != 1
        || func.sig.returns[0].ty != Type::I64
        || func
            .sig
            .params
            .iter()
            .chain(&func.sig.returns)
            .any(|p| !matches!(p.ty, Type::I64 | Type::Ptr) || p.purpose != Purpose::Normal)
        || func.num_stack_slots() != 0
        || !func.debug_vars.is_empty()
    {
        return None;
    }
    let extra = func
        .blocks()
        .map(|b| func.block_params(b).len())
        .max()
        .unwrap_or(0);
    let frame = func
        .num_values()
        .checked_add(extra)?
        .checked_mul(8)?
        .checked_add(15)?
        & !15;
    // One ADD/SUB immediate and scaled stack accesses. Larger frames fall back.
    if frame > 4080 {
        return None;
    }
    let frame = frame as u32;
    let mut bytes = Vec::with_capacity(func.num_values() * 24 + 64);
    let mut epilogues = Vec::new();
    word(&mut bytes, 0xd10003ff | frame << 10);
    let entry = func.entry_block()?;
    for (index, &value) in func.block_params(entry).iter().enumerate() {
        store(&mut bytes, index as u32, home(func, value));
    }
    let mut offsets = vec![0; func.num_blocks()];
    let mut branches = Vec::new();
    for block in func.blocks() {
        offsets[block.index()] = bytes.len();
        if func
            .block_params(block)
            .iter()
            .any(|&v| !matches!(func.value_type(v), Type::I64 | Type::I8 | Type::Ptr))
        {
            return None;
        }
        for &inst in func.block_insts(block) {
            let dst = func
                .inst_results(inst)
                .first()
                .map_or(0, |&v| home(func, v));
            match func.inst_data(inst) {
                InstData::Iconst { ty: Type::I64, imm } => {
                    for half in 0..4 {
                        word(
                            &mut bytes,
                            (if half == 0 { 0xd2800009 } else { 0xf2800009 })
                                | half << 21
                                | (((*imm as u64) >> (half * 16)) as u32 & 0xffff) << 5,
                        );
                    }
                    store(&mut bytes, 9, dst);
                }
                InstData::Binary { op, args } => {
                    if !matches!(func.value_type(args[0]), Type::I64 | Type::Ptr)
                        || func.value_type(args[1]) != Type::I64
                    {
                        return None;
                    }
                    let opcode = match op {
                        BinaryOp::Iadd | BinaryOp::Ptradd => 0x8b0a0129,
                        BinaryOp::Isub => 0xcb0a0129,
                        BinaryOp::Imul => 0x9b0a7d29,
                        BinaryOp::Xor => 0xca0a0129,
                        BinaryOp::And => 0x8a0a0129,
                        BinaryOp::Or => 0xaa0a0129,
                        BinaryOp::Shl => 0x9aca2129,
                        BinaryOp::Ushr => 0x9aca2529,
                        _ => return None,
                    };
                    load(&mut bytes, 9, home(func, args[0]));
                    load(&mut bytes, 10, home(func, args[1]));
                    word(&mut bytes, opcode);
                    store(&mut bytes, 9, dst);
                }
                InstData::Icmp { cond, args } if func.value_type(args[0]) == Type::I64 => {
                    let condition: u32 = match cond {
                        IntCC::Eq => 0,
                        IntCC::Ne => 1,
                        IntCC::Ult => 3,
                        IntCC::Ule => 9,
                        IntCC::Ugt => 8,
                        IntCC::Uge => 2,
                        IntCC::Slt => 11,
                        IntCC::Sle => 13,
                        IntCC::Sgt => 12,
                        IntCC::Sge => 10,
                    };
                    load(&mut bytes, 9, home(func, args[0]));
                    load(&mut bytes, 10, home(func, args[1]));
                    word(&mut bytes, 0xeb0a013f);
                    word(&mut bytes, 0x9a9f07e9 | (condition ^ 1) << 12);
                    store(&mut bytes, 9, dst);
                }
                InstData::Load {
                    ty: Type::I64,
                    addr,
                    ..
                } => {
                    load(&mut bytes, 9, home(func, *addr));
                    word(&mut bytes, 0xf9400129);
                    store(&mut bytes, 9, dst);
                }
                InstData::Store { value, addr, .. } if func.value_type(*value) == Type::I64 => {
                    load(&mut bytes, 9, home(func, *addr));
                    load(&mut bytes, 10, home(func, *value));
                    word(&mut bytes, 0xf900012a);
                }
                InstData::Jump { dest } => edge(func, dest, &mut bytes, &mut branches),
                InstData::Brif { cond, dests } => {
                    load(&mut bytes, 9, home(func, *cond));
                    let position = bytes.len();
                    word(&mut bytes, 0xb4000009);
                    edge(func, &dests[0], &mut bytes, &mut branches);
                    let distance = (bytes.len() - position) / 4;
                    if distance >= 1 << 18 {
                        return None;
                    }
                    patch(&mut bytes, position, 0xb4000009 | (distance as u32) << 5);
                    edge(func, &dests[1], &mut bytes, &mut branches);
                }
                InstData::Return { args } if args.len() == 1 => {
                    load(&mut bytes, 0, home(func, args[0]));
                    epilogues.push(bytes.len());
                    word(&mut bytes, 0x910003ff | frame << 10);
                    word(&mut bytes, 0xd65f03c0);
                }
                _ => return None,
            }
        }
    }
    for (position, block) in branches {
        let distance = (offsets[block] as i64 - position as i64) / 4;
        if !(-(1 << 25)..1 << 25).contains(&distance) {
            return None;
        }
        patch(
            &mut bytes,
            position,
            0x14000000 | (distance as u32 & 0x3ffffff),
        );
    }
    Some(Code {
        bytes,
        frame,
        epilogues,
    })
}

pub(super) fn emit(
    func: &Function,
    out: &mut Output,
    linux: bool,
    emission: crate::EmissionOptions,
) -> bool {
    let Some(code) = encode(func) else {
        return false;
    };
    super::function_entry(func, out, linux, emission.function_sections);
    let unwind = emission.unwind_tables == crate::UnwindTables::All;
    if unwind {
        out.directive(Directive::Cfi(Cfi::Start));
    }
    // CFI describes the state at each instruction boundary, including later
    // laid-out blocks after a return. No assembly or instruction re-encoding runs.
    out.bytes(&code.bytes[..4]);
    if unwind {
        out.directive(Directive::Cfi(Cfi::DefCfaOffset(code.frame.into())));
    }
    let mut position = 4;
    for epilogue in code.epilogues {
        out.bytes(&code.bytes[position..epilogue + 4]);
        if unwind {
            out.directive(Directive::Cfi(Cfi::DefCfaOffset(0)));
        }
        out.bytes(&code.bytes[epilogue + 4..epilogue + 8]);
        if unwind {
            out.directive(Directive::Cfi(Cfi::DefCfaOffset(code.frame.into())));
        }
        position = epilogue + 8;
    }
    out.bytes(&code.bytes[position..]);
    let end = format!(
        "{}{name}_end",
        if linux { ".L" } else { "L" },
        name = func.name
    );
    out.label(end.clone());
    if linux {
        out.directive(Directive::Size {
            name: func.name.clone(),
            expression: Expression::difference(end, &func.name),
        });
    }
    if unwind {
        out.directive(Directive::Cfi(Cfi::End));
    }
    crate::backend_profile::count(crate::backend_profile::Counter::DirectFunctions);
    true
}
