//! Bounded SysV scalar functions encoded without selection, liveness or allocation.
//! Every value has a zero-extended eight-byte home. rax/rcx/rdx compute values;
//! r10 holds indirect call targets. Failed eligibility publishes no partial output.
use super::{Cfi, Directive, Expression, Function, Output, Sink};
use keel_ir::{
    ArgExt, BinaryOp, BlockCall, CallArgs, ConvertOp, EntityRef, FuncRef, InstData, IntCC,
    Intrinsic, Purpose, Signature, Type, UnaryOp, Value,
};

fn width(ty: Type) -> Option<u32> {
    Some(match ty {
        Type::I8 => 8,
        Type::I16 => 16,
        Type::I32 => 32,
        Type::I64 | Type::Ptr => 64,
        _ => return None,
    })
}
fn integer(ty: Type) -> bool {
    matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64)
}
fn home(func: &Function, value: Value) -> u32 {
    (func.resolve(value).index() * 8) as u32
}
fn stack(out: &mut Vec<u8>, opcode: u8, reg: u8, offset: u32) {
    out.extend_from_slice(&[
        0x48 | ((reg >> 3) << 2),
        opcode,
        0x84 | ((reg & 7) << 3),
        0x24,
    ]);
    out.extend_from_slice(&offset.to_le_bytes());
}
fn load(out: &mut Vec<u8>, reg: u8, offset: u32) {
    stack(out, 0x8b, reg, offset);
}
fn store(out: &mut Vec<u8>, reg: u8, offset: u32) {
    stack(out, 0x89, reg, offset);
}
fn normalize(out: &mut Vec<u8>, reg: u8, bits: u32) {
    match bits {
        8 => out.extend_from_slice(&[0x0f, 0xb6, 0xc0 | reg << 3 | reg]),
        16 => out.extend_from_slice(&[0x0f, 0xb7, 0xc0 | reg << 3 | reg]),
        32 => out.extend_from_slice(&[0x89, 0xc0 | reg << 3 | reg]),
        64 => {}
        _ => unreachable!(),
    }
}
fn signed(out: &mut Vec<u8>, reg: u8, bits: u32) {
    match bits {
        8 => out.extend_from_slice(&[0x48, 0x0f, 0xbe, 0xc0 | reg << 3 | reg]),
        16 => out.extend_from_slice(&[0x48, 0x0f, 0xbf, 0xc0 | reg << 3 | reg]),
        32 => out.extend_from_slice(&[0x48, 0x63, 0xc0 | reg << 3 | reg]),
        64 => {}
        _ => unreachable!(),
    }
}
fn result(func: &Function, out: &mut Vec<u8>, value: Value) -> Option<()> {
    normalize(out, 0, width(func.value_type(value))?);
    store(out, 0, home(func, value));
    Some(())
}
fn edge(func: &Function, dest: &BlockCall, out: &mut Vec<u8>, branches: &mut Vec<(usize, usize)>) {
    let temporary = (func.num_values() * 8) as u32;
    for (i, &value) in dest.args.iter().enumerate() {
        load(out, 0, home(func, value));
        store(out, 0, temporary + i as u32 * 8);
    }
    for (i, &value) in func.block_params(dest.block).iter().enumerate() {
        load(out, 0, temporary + i as u32 * 8);
        store(out, 0, home(func, value));
    }
    out.push(0xe9);
    branches.push((out.len(), dest.block.index()));
    out.extend_from_slice(&[0; 4]);
}
fn displacement(out: &mut [u8], position: usize, target: usize) -> Option<()> {
    let delta = i32::try_from(target as i64 - position as i64 - 4).ok()?;
    out[position..position + 4].copy_from_slice(&delta.to_le_bytes());
    Some(())
}
struct CallSite {
    offset: usize,
    callee: FuncRef,
}
struct Code {
    bytes: Vec<u8>,
    frame: u32,
    epilogues: Vec<usize>,
    calls: Vec<CallSite>,
}
fn scalar_signature(sig: &Signature) -> bool {
    !sig.variadic
        && !sig.returns_twice
        && sig.params.len() <= 6
        && sig.returns.len() <= 1
        && sig.x86_vector_abi.is_none()
        && sig
            .params
            .iter()
            .chain(&sig.returns)
            .all(|p| width(p.ty).is_some() && p.purpose == Purpose::Normal)
}
fn arguments(func: &Function, args: &CallArgs, sig: &Signature, bytes: &mut Vec<u8>) -> Option<()> {
    if !scalar_signature(sig) || args.has_explicit_abi() || args.len() != sig.params.len() {
        return None;
    }
    for ((&value, param), reg) in args.iter().zip(&sig.params).zip([7, 6, 2, 1, 8, 9]) {
        if func.value_type(value) != param.ty {
            return None;
        }
        load(bytes, 0, home(func, value));
        if param.ext == ArgExt::Sext {
            signed(bytes, 0, width(param.ty)?);
        }
        // All values already have canonical homes. rax is outside the argument bank.
        bytes.extend_from_slice(&[0x48 | (reg >> 3), 0x89, 0xc0 | (reg & 7)]);
    }
    Some(())
}
fn encode(func: &Function) -> Option<Code> {
    if !scalar_signature(&func.sig) || func.personality.is_some() || !func.debug_vars.is_empty() {
        return None;
    }
    let entry = func.entry_block()?;
    let extra = func
        .blocks()
        .map(|b| func.block_params(b).len())
        .max()
        .unwrap_or(0);
    let mut used = func.num_values().checked_add(extra)?.checked_mul(8)?;
    let mut slots = vec![0; func.num_stack_slots()];
    for (slot, data) in func.stack_slots() {
        if data.align == 0 || !data.align.is_power_of_two() || data.align > 16 {
            return None;
        }
        used = used.checked_add(data.align as usize - 1)? & !(data.align as usize - 1);
        slots[slot.index()] = u32::try_from(used).ok()?;
        used = used.checked_add(data.size as usize)?;
    }
    // Entry rsp is 8 mod 16. Allocating 8 mod 16 makes fixed slots 16-aligned.
    let frame = (used.checked_add(8)?.checked_add(15)? & !15).checked_sub(8)?;
    if frame > 4088 {
        return None;
    }
    let frame = frame as u32;
    let mut bytes = Vec::with_capacity(func.num_values() * 32 + 64);
    let mut epilogues = Vec::new();
    let mut calls = Vec::new();
    bytes.extend_from_slice(&[0x48, 0x81, 0xec]);
    bytes.extend_from_slice(&frame.to_le_bytes());
    for ((&value, param), reg) in func
        .block_params(entry)
        .iter()
        .zip(&func.sig.params)
        .zip([7, 6, 2, 1, 8, 9])
    {
        // rax is not an argument register, so normalizing cannot clobber later arguments.
        bytes.extend_from_slice(&[0x48 | ((reg >> 3) << 2), 0x89, 0xc0 | ((reg & 7) << 3)]);
        normalize(&mut bytes, 0, width(param.ty)?);
        store(&mut bytes, 0, home(func, value));
    }
    let mut offsets = vec![0; func.num_blocks()];
    let mut branches = Vec::new();
    for block in func.blocks() {
        offsets[block.index()] = bytes.len();
        if func
            .block_params(block)
            .iter()
            .any(|&v| width(func.value_type(v)).is_none())
        {
            return None;
        }
        for &inst in func.block_insts(block) {
            let values = func.inst_results(inst);
            let dst = values.first().copied();
            match func.inst_data(inst) {
                InstData::Iconst { ty, imm } if integer(*ty) => {
                    let bits = width(*ty)?;
                    let immediate = (*imm as u64) & (u64::MAX >> (64 - bits));
                    bytes.extend_from_slice(&[0x48, 0xb8]);
                    bytes.extend_from_slice(&immediate.to_le_bytes());
                    store(&mut bytes, 0, home(func, dst?));
                }
                InstData::Binary { op, args } => {
                    let ty = func.value_type(args[0]);
                    let bits = width(ty)?;
                    width(func.value_type(args[1]))?;
                    if !integer(ty) && *op != BinaryOp::Ptradd {
                        return None;
                    }
                    load(&mut bytes, 0, home(func, args[0]));
                    load(&mut bytes, 1, home(func, args[1]));
                    match op {
                        BinaryOp::Iadd | BinaryOp::Ptradd => {
                            bytes.extend_from_slice(&[0x48, 0x01, 0xc8])
                        }
                        BinaryOp::Isub => bytes.extend_from_slice(&[0x48, 0x29, 0xc8]),
                        BinaryOp::Imul => bytes.extend_from_slice(&[0x48, 0x0f, 0xaf, 0xc1]),
                        BinaryOp::And => bytes.extend_from_slice(&[0x48, 0x21, 0xc8]),
                        BinaryOp::Or => bytes.extend_from_slice(&[0x48, 0x09, 0xc8]),
                        BinaryOp::Xor => bytes.extend_from_slice(&[0x48, 0x31, 0xc8]),
                        BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                            if *op == BinaryOp::Sshr {
                                signed(&mut bytes, 0, bits);
                            }
                            // The IR masks by value width even when the count has another width.
                            if bits < 64 {
                                bytes.extend_from_slice(&[0x83, 0xe1, (bits - 1) as u8]);
                            }
                            bytes.extend_from_slice(&[
                                0x48,
                                0xd3,
                                match op {
                                    BinaryOp::Shl => 0xe0,
                                    BinaryOp::Ushr => 0xe8,
                                    _ => 0xf8,
                                },
                            ]);
                        }
                        _ => return None,
                    }
                    result(func, &mut bytes, dst?)?;
                }
                InstData::Unary { op, arg } if integer(func.value_type(*arg)) => {
                    load(&mut bytes, 0, home(func, *arg));
                    bytes.extend_from_slice(&[
                        0x48,
                        0xf7,
                        match op {
                            UnaryOp::Ineg => 0xd8,
                            UnaryOp::Bnot => 0xd0,
                            _ => return None,
                        },
                    ]);
                    result(func, &mut bytes, dst?)?;
                }
                InstData::Convert { op, ty, arg } => {
                    let source = width(func.value_type(*arg))?;
                    width(*ty)?;
                    load(&mut bytes, 0, home(func, *arg));
                    match op {
                        ConvertOp::Sext => signed(&mut bytes, 0, source),
                        ConvertOp::Uext
                        | ConvertOp::Trunc
                        | ConvertOp::PtrToInt
                        | ConvertOp::IntToPtr => {}
                        _ => return None,
                    }
                    result(func, &mut bytes, dst?)?;
                }
                InstData::Icmp { cond, args } => {
                    let ty = func.value_type(args[0]);
                    let bits = width(ty)?;
                    load(&mut bytes, 0, home(func, args[0]));
                    load(&mut bytes, 1, home(func, args[1]));
                    if matches!(cond, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge) {
                        signed(&mut bytes, 0, bits);
                        signed(&mut bytes, 1, bits);
                    }
                    let condition = match cond {
                        IntCC::Eq => 4,
                        IntCC::Ne => 5,
                        IntCC::Ult => 2,
                        IntCC::Ule => 6,
                        IntCC::Ugt => 7,
                        IntCC::Uge => 3,
                        IntCC::Slt => 12,
                        IntCC::Sle => 14,
                        IntCC::Sgt => 15,
                        IntCC::Sge => 13,
                    };
                    bytes.extend_from_slice(&[
                        0x48,
                        0x39,
                        0xc8,
                        0x0f,
                        0x90 | condition,
                        0xc0,
                        0x0f,
                        0xb6,
                        0xc0,
                    ]);
                    store(&mut bytes, 0, home(func, dst?));
                }
                InstData::Select { args } => {
                    width(func.value_type(args[1]))?;
                    load(&mut bytes, 0, home(func, args[0]));
                    load(&mut bytes, 1, home(func, args[1]));
                    load(&mut bytes, 2, home(func, args[2]));
                    bytes.extend_from_slice(&[
                        0x48, 0x85, 0xc0, 0x48, 0x0f, 0x44, 0xca, 0x48, 0x89, 0xc8,
                    ]);
                    store(&mut bytes, 0, home(func, dst?));
                }
                InstData::Load { ty, addr, .. } => {
                    let bits = width(*ty)?;
                    load(&mut bytes, 0, home(func, *addr));
                    bytes.extend_from_slice(match bits {
                        8 => &[0x0f, 0xb6, 0x00][..],
                        16 => &[0x0f, 0xb7, 0x00][..],
                        32 => &[0x8b, 0x00][..],
                        _ => &[0x48, 0x8b, 0x00][..],
                    });
                    store(&mut bytes, 0, home(func, dst?));
                }
                InstData::Store { value, addr, .. } => {
                    let bits = width(func.value_type(*value))?;
                    load(&mut bytes, 0, home(func, *addr));
                    load(&mut bytes, 1, home(func, *value));
                    bytes.extend_from_slice(match bits {
                        8 => &[0x88, 0x08][..],
                        16 => &[0x66, 0x89, 0x08][..],
                        32 => &[0x89, 0x08][..],
                        _ => &[0x48, 0x89, 0x08][..],
                    });
                }
                InstData::StackAddr { slot } => {
                    bytes.extend_from_slice(&[0x48, 0x8d, 0x84, 0x24]);
                    bytes.extend_from_slice(&slots[slot.index()].to_le_bytes());
                    store(&mut bytes, 0, home(func, dst?));
                }
                InstData::Call { func: callee, args } => {
                    let sig = &func.callee(*callee).sig;
                    if values.len() != sig.returns.len() {
                        return None;
                    }
                    arguments(func, args, sig, &mut bytes)?;
                    calls.push(CallSite {
                        offset: bytes.len(),
                        callee: *callee,
                    });
                    // Publishing replaces exactly these five bytes with a symbolic CALL.
                    bytes.extend_from_slice(&[0xe8, 0, 0, 0, 0]);
                    if let Some(value) = dst {
                        result(func, &mut bytes, value)?;
                    }
                }
                InstData::CallIndirect { sig, callee, args } => {
                    let sig = func.signature(*sig);
                    if func.value_type(*callee) != Type::Ptr || values.len() != sig.returns.len() {
                        return None;
                    }
                    arguments(func, args, sig, &mut bytes)?;
                    load(&mut bytes, 10, home(func, *callee));
                    bytes.extend_from_slice(&[0x41, 0xff, 0xd2]);
                    if let Some(value) = dst {
                        result(func, &mut bytes, value)?;
                    }
                }
                InstData::Intrinsic {
                    id: Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd,
                    ..
                } => {}
                InstData::Jump { dest } => edge(func, dest, &mut bytes, &mut branches),
                InstData::Brif { cond, dests } => {
                    width(func.value_type(*cond))?;
                    load(&mut bytes, 0, home(func, *cond));
                    bytes.extend_from_slice(&[0x48, 0x85, 0xc0, 0x0f, 0x84]);
                    let position = bytes.len();
                    bytes.extend_from_slice(&[0; 4]);
                    edge(func, &dests[0], &mut bytes, &mut branches);
                    let target = bytes.len();
                    displacement(&mut bytes, position, target)?;
                    edge(func, &dests[1], &mut bytes, &mut branches);
                }
                InstData::Return { args } if args.len() == func.sig.returns.len() => {
                    if let Some(&arg) = args.first() {
                        load(&mut bytes, 0, home(func, arg));
                        if func.sig.returns[0].ext == ArgExt::Sext {
                            signed(&mut bytes, 0, width(func.value_type(arg))?);
                        }
                    }
                    epilogues.push(bytes.len());
                    bytes.extend_from_slice(&[0x48, 0x81, 0xc4]);
                    bytes.extend_from_slice(&frame.to_le_bytes());
                    bytes.push(0xc3);
                }
                _ => return None,
            }
        }
    }
    for (position, block) in branches {
        displacement(&mut bytes, position, offsets[block])?;
    }
    Some(Code {
        bytes,
        frame,
        epilogues,
        calls,
    })
}

fn body(
    func: &Function,
    symbols: &crate::symbols::Symbols<'_>,
    out: &mut Output,
    bytes: &[u8],
    position: &mut usize,
    end: usize,
    calls: &mut std::iter::Peekable<std::slice::Iter<'_, CallSite>>,
) {
    while calls.peek().is_some_and(|call| call.offset < end) {
        let call = calls.next().expect("peeked call");
        out.bytes(&bytes[*position..call.offset]);
        let name = &func.callee(call.callee).name;
        out.branch_to_function(keel_asm_ir::x86::Opcode::Call, name, !symbols.defines(name));
        *position = call.offset + 5;
    }
    out.bytes(&bytes[*position..end]);
    *position = end;
}

pub(super) fn emit(
    func: &Function,
    symbols: &crate::symbols::Symbols<'_>,
    out: &mut Output,
    emission: crate::EmissionOptions,
) -> bool {
    let Some(code) = encode(func) else {
        return false;
    };
    super::function_entry(
        func,
        out,
        emission.function_sections,
        crate::emission::function_goal(&func.attrs, emission.goal),
    );
    let unwind = emission.unwind_tables == crate::UnwindTables::All;
    if unwind {
        out.directive(Directive::Cfi(Cfi::Start));
        out.directive(Directive::Cfi(Cfi::DefCfa {
            register: 7,
            offset: 8,
        }));
    }
    out.bytes(&code.bytes[..7]);
    if unwind {
        out.directive(Directive::Cfi(Cfi::DefCfaOffset(u64::from(code.frame) + 8)));
    }
    let mut position = 7;
    let mut calls = code.calls.iter().peekable();
    for epilogue in code.epilogues {
        body(
            func,
            symbols,
            out,
            &code.bytes,
            &mut position,
            epilogue + 7,
            &mut calls,
        );
        if unwind {
            out.directive(Directive::Cfi(Cfi::DefCfaOffset(8)));
        }
        out.bytes(&code.bytes[epilogue + 7..epilogue + 8]);
        if unwind {
            out.directive(Directive::Cfi(Cfi::DefCfaOffset(u64::from(code.frame) + 8)));
        }
        position = epilogue + 8;
    }
    body(
        func,
        symbols,
        out,
        &code.bytes,
        &mut position,
        code.bytes.len(),
        &mut calls,
    );
    out.label(format!(".L{}_end", func.name));
    if unwind {
        out.directive(Directive::Cfi(Cfi::End));
    }
    out.directive(Directive::Size {
        name: func.name.clone(),
        expression: Expression::difference(".", &func.name),
    });
    crate::backend_profile::count(crate::backend_profile::Counter::DirectFunctions);
    true
}

#[cfg(test)]
mod tests {
    use super::{load, normalize, signed, store};
    use object::{Object, ObjectSection};
    #[test]
    fn scratch_encodings_match_the_assembler() {
        let mut cases = Vec::new();
        let mut code = Vec::new();
        load(&mut code, 0, 256);
        cases.push((code, "mov rax,qword ptr [rsp+256]"));
        let mut code = Vec::new();
        store(&mut code, 8, 256);
        cases.push((code, "mov qword ptr [rsp+256],r8"));
        for (reg, bits, asm) in [
            (0, 8, "movzx eax,al"),
            (1, 16, "movzx ecx,cx"),
            (2, 32, "mov edx,edx"),
        ] {
            let mut code = Vec::new();
            normalize(&mut code, reg, bits);
            cases.push((code, asm));
        }
        for (reg, bits, asm) in [
            (0, 8, "movsx rax,al"),
            (1, 16, "movsx rcx,cx"),
            (2, 32, "movsxd rdx,edx"),
        ] {
            let mut code = Vec::new();
            signed(&mut code, reg, bits);
            cases.push((code, asm));
        }
        for (code, instruction) in cases {
            let object = keel_asm::x86::assemble_object(&format!(
                ".text\n.intel_syntax noprefix\n{instruction}\n"
            ))
            .unwrap();
            let file = object::File::parse(object.as_slice()).unwrap();
            assert_eq!(
                code,
                file.section_by_name(".text").unwrap().data().unwrap(),
                "{instruction}"
            );
        }
    }
}
