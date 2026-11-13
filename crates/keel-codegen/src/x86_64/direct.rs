//! Bounded SysV scalar functions encoded without selection, liveness or allocation.
//! Every value has a zero-extended eight-byte home. rax/rcx/rdx compute values;
//! r10 holds indirect call targets. Failed eligibility publishes no partial output.
use super::{
    Cfi, CodeModel, Directive, Expression, Function, Output, Sink, asm_arg, r32, r64, rip, sized,
    symbol_address, symbol_memory,
};
use keel_ir::{
    ArgExt, BinaryOp, BlockCall, CallArgs, ConvertOp, EntityRef, FuncRef, InstData, IntCC,
    Intrinsic, Purpose, Signature, Symbol, Type, UnaryOp, Value,
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
fn home(func: &Function, base: u32, value: Value) -> u32 {
    base + (func.resolve(value).index() * 8) as u32
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
fn result(func: &Function, base: u32, out: &mut Vec<u8>, value: Value) -> Option<()> {
    normalize(out, 0, width(func.value_type(value))?);
    store(out, 0, home(func, base, value));
    Some(())
}
fn edge(
    func: &Function,
    base: u32,
    dest: &BlockCall,
    out: &mut Vec<u8>,
    branches: &mut Vec<(usize, usize)>,
) {
    let temporary = base + (func.num_values() * 8) as u32;
    for (i, &value) in dest.args.iter().enumerate() {
        load(out, 0, home(func, base, value));
        store(out, 0, temporary + i as u32 * 8);
    }
    for (i, &value) in func.block_params(dest.block).iter().enumerate() {
        load(out, 0, temporary + i as u32 * 8);
        store(out, 0, home(func, base, value));
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
enum Symbolic {
    Call(FuncRef),
    Memory(&'static str),
    Address {
        symbol: Symbol,
        model: CodeModel,
        defined: bool,
    },
}
impl Symbolic {
    fn length(&self) -> usize {
        match self {
            Self::Call(_) | Self::Memory(_) => 5,
            Self::Address {
                model: CodeModel::Small,
                ..
            } => 5,
            Self::Address { .. } => 7,
        }
    }
}
struct Site {
    offset: usize,
    operation: Symbolic,
}
struct Code {
    bytes: Vec<u8>,
    frame: u32,
    epilogues: Vec<usize>,
    sites: Vec<Site>,
}
fn result_registers(sig: &Signature) -> Option<Vec<u8>> {
    crate::sysv::multi_result_locs(sig)
        .ok()?
        .into_iter()
        .map(|loc| {
            if let crate::regalloc::Loc::Reg(reg) = loc {
                Some(reg)
            } else {
                None
            }
        })
        .collect()
}
fn capture_results(
    func: &Function,
    base: u32,
    values: &[Value],
    sig: &Signature,
    bytes: &mut Vec<u8>,
) -> Option<()> {
    if sig.returns.len() <= 1 {
        return if let Some(&value) = values.first() {
            result(func, base, bytes, value)
        } else {
            Some(())
        };
    }
    for ((&value, param), reg) in values.iter().zip(&sig.returns).zip(result_registers(sig)?) {
        if func.value_type(value) != param.ty {
            return None;
        }
        // Capture RAX first, then normalize each other result through RAX.
        // The remaining result registers stay intact, including R8 and R9.
        if reg != 0 {
            bytes.extend_from_slice(&[0x48 | ((reg >> 3) << 2), 0x89, 0xc0 | ((reg & 7) << 3)]);
        }
        result(func, base, bytes, value)?;
    }
    Some(())
}
fn scalar_signature(sig: &Signature) -> bool {
    !sig.variadic
        && !sig.returns_twice
        && (sig.returns.len() <= 1 || result_registers(sig).is_some())
        && sig.x86_vector_abi.is_none()
        && sig
            .params
            .iter()
            .chain(&sig.returns)
            .all(|p| width(p.ty).is_some() && p.purpose == Purpose::Normal)
}
fn arguments(
    func: &Function,
    base: u32,
    args: &CallArgs,
    sig: &Signature,
    bytes: &mut Vec<u8>,
) -> Option<()> {
    if !scalar_signature(sig) || args.has_explicit_abi() || args.len() != sig.params.len() {
        return None;
    }
    for (index, (&value, param)) in args.iter().zip(&sig.params).enumerate() {
        if func.value_type(value) != param.ty {
            return None;
        }
        load(bytes, 0, home(func, base, value));
        if param.ext == ArgExt::Sext {
            signed(bytes, 0, width(param.ty)?);
        }
        // Outgoing slots are below every SSA home and edge-copy temporary.
        // rax is outside the argument bank, so argument copies cannot cycle.
        if let Some(&reg) = [7, 6, 2, 1, 8, 9].get(index) {
            bytes.extend_from_slice(&[0x48 | (reg >> 3), 0x89, 0xc0 | (reg & 7)]);
        } else {
            store(bytes, 0, u32::try_from((index - 6).checked_mul(8)?).ok()?);
        }
    }
    Some(())
}
fn encode(
    func: &Function,
    symbols: &crate::symbols::Symbols<'_>,
    model: CodeModel,
) -> Option<Code> {
    if !scalar_signature(&func.sig) || func.personality.is_some() || !func.debug_vars.is_empty() {
        return None;
    }
    let mut outgoing = 0usize;
    for (_, inst) in func.layout_insts() {
        let sig = match func.inst_data(inst) {
            InstData::Call { func: callee, .. } => &func.callee(*callee).sig,
            InstData::CallIndirect { sig, .. } => func.signature(*sig),
            _ => continue,
        };
        if !scalar_signature(sig) {
            return None;
        }
        outgoing = outgoing.max(sig.params.len().saturating_sub(6).checked_mul(8)?);
    }
    let base = u32::try_from(outgoing).ok()?;
    let entry = func.entry_block()?;
    let extra = func
        .blocks()
        .map(|b| func.block_params(b).len())
        .max()
        .unwrap_or(0);
    let mut used = outgoing.checked_add(func.num_values().checked_add(extra)?.checked_mul(8)?)?;
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
    if frame > i32::MAX as usize / 2 {
        return None;
    }
    let incoming = func.sig.params.len().saturating_sub(6).checked_mul(8)?;
    if frame.checked_add(8)?.checked_add(incoming)? > i32::MAX as usize {
        return None;
    }
    let frame = frame as u32;
    let mut bytes = Vec::with_capacity(func.num_values() * 32 + 64);
    let mut epilogues = Vec::new();
    let mut sites = Vec::new();
    let mut traps = Vec::new();
    bytes.extend_from_slice(&[0x48, 0x81, 0xec]);
    bytes.extend_from_slice(&frame.to_le_bytes());
    for (index, (&value, param)) in func
        .block_params(entry)
        .iter()
        .zip(&func.sig.params)
        .enumerate()
    {
        // rsp+frame is the original return address; scalar stack arguments
        // begin eight bytes above it, in slots owned by the caller.
        if let Some(&reg) = [7, 6, 2, 1, 8, 9].get(index) {
            bytes.extend_from_slice(&[0x48 | ((reg >> 3) << 2), 0x89, 0xc0 | ((reg & 7) << 3)]);
        } else {
            let offset = (frame as usize)
                .checked_add(8)?
                .checked_add((index - 6).checked_mul(8)?)?;
            load(&mut bytes, 0, u32::try_from(offset).ok()?);
        }
        normalize(&mut bytes, 0, width(param.ty)?);
        store(&mut bytes, 0, home(func, base, value));
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
                    store(&mut bytes, 0, home(func, base, dst?));
                }
                InstData::Binary { op, args } => {
                    let ty = func.value_type(args[0]);
                    let bits = width(ty)?;
                    width(func.value_type(args[1]))?;
                    if !integer(ty) && *op != BinaryOp::Ptradd {
                        return None;
                    }
                    load(&mut bytes, 0, home(func, base, args[0]));
                    load(&mut bytes, 1, home(func, base, args[1]));
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
                        BinaryOp::Udiv | BinaryOp::Sdiv | BinaryOp::Urem | BinaryOp::Srem => {
                            let signed_div = matches!(op, BinaryOp::Sdiv | BinaryOp::Srem);
                            let remainder = matches!(op, BinaryOp::Urem | BinaryOp::Srem);
                            if signed_div {
                                signed(&mut bytes, 0, bits);
                                signed(&mut bytes, 1, bits);
                            }
                            bytes.extend_from_slice(&[0x48, 0x85, 0xc9, 0x0f, 0x84]);
                            traps.push(bytes.len());
                            bytes.extend_from_slice(&[0; 4]);
                            let mut done = None;
                            if signed_div {
                                bytes.extend_from_slice(&[0x48, 0x83, 0xf9, 0xff, 0x0f, 0x85]);
                                let divide = bytes.len();
                                bytes.extend_from_slice(&[0; 4]);
                                if remainder {
                                    // A % -1 is zero, including MIN % -1, where IDIV would fault.
                                    bytes.extend_from_slice(&[0x31, 0xd2, 0xe9]);
                                    done = Some(bytes.len());
                                    bytes.extend_from_slice(&[0; 4]);
                                } else {
                                    bytes.extend_from_slice(&[0x48, 0xba]);
                                    let min = -(1i128 << (bits - 1));
                                    bytes.extend_from_slice(&(min as i64).to_le_bytes());
                                    bytes.extend_from_slice(&[0x48, 0x39, 0xd0, 0x0f, 0x84]);
                                    traps.push(bytes.len());
                                    bytes.extend_from_slice(&[0; 4]);
                                }
                                let target = bytes.len();
                                displacement(&mut bytes, divide, target)?;
                                bytes.extend_from_slice(&[0x48, 0x99, 0x48, 0xf7, 0xf9]);
                            } else {
                                bytes.extend_from_slice(&[0x31, 0xd2, 0x48, 0xf7, 0xf1]);
                            }
                            if let Some(done) = done {
                                let target = bytes.len();
                                displacement(&mut bytes, done, target)?;
                            }
                            if remainder {
                                bytes.extend_from_slice(&[0x48, 0x89, 0xd0]);
                            }
                        }
                        _ => return None,
                    }
                    result(func, base, &mut bytes, dst?)?;
                }
                InstData::Unary { op, arg } if integer(func.value_type(*arg)) => {
                    load(&mut bytes, 0, home(func, base, *arg));
                    bytes.extend_from_slice(&[
                        0x48,
                        0xf7,
                        match op {
                            UnaryOp::Ineg => 0xd8,
                            UnaryOp::Bnot => 0xd0,
                            _ => return None,
                        },
                    ]);
                    result(func, base, &mut bytes, dst?)?;
                }
                InstData::Convert { op, ty, arg } => {
                    let source = width(func.value_type(*arg))?;
                    width(*ty)?;
                    load(&mut bytes, 0, home(func, base, *arg));
                    match op {
                        ConvertOp::Sext => signed(&mut bytes, 0, source),
                        ConvertOp::Uext
                        | ConvertOp::Trunc
                        | ConvertOp::PtrToInt
                        | ConvertOp::IntToPtr => {}
                        _ => return None,
                    }
                    result(func, base, &mut bytes, dst?)?;
                }
                InstData::Icmp { cond, args } => {
                    let ty = func.value_type(args[0]);
                    let bits = width(ty)?;
                    load(&mut bytes, 0, home(func, base, args[0]));
                    load(&mut bytes, 1, home(func, base, args[1]));
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
                    store(&mut bytes, 0, home(func, base, dst?));
                }
                InstData::Select { args } => {
                    width(func.value_type(args[1]))?;
                    load(&mut bytes, 0, home(func, base, args[0]));
                    load(&mut bytes, 1, home(func, base, args[1]));
                    load(&mut bytes, 2, home(func, base, args[2]));
                    bytes.extend_from_slice(&[
                        0x48, 0x85, 0xc0, 0x48, 0x0f, 0x44, 0xca, 0x48, 0x89, 0xc8,
                    ]);
                    store(&mut bytes, 0, home(func, base, dst?));
                }
                InstData::Load { ty, addr, .. } => {
                    let bits = width(*ty)?;
                    load(&mut bytes, 0, home(func, base, *addr));
                    bytes.extend_from_slice(match bits {
                        8 => &[0x0f, 0xb6, 0x00][..],
                        16 => &[0x0f, 0xb7, 0x00][..],
                        32 => &[0x8b, 0x00][..],
                        _ => &[0x48, 0x8b, 0x00][..],
                    });
                    store(&mut bytes, 0, home(func, base, dst?));
                }
                InstData::Store { value, addr, .. } => {
                    let bits = width(func.value_type(*value))?;
                    load(&mut bytes, 0, home(func, base, *addr));
                    load(&mut bytes, 1, home(func, base, *value));
                    bytes.extend_from_slice(match bits {
                        8 => &[0x88, 0x08][..],
                        16 => &[0x66, 0x89, 0x08][..],
                        32 => &[0x89, 0x08][..],
                        _ => &[0x48, 0x89, 0x08][..],
                    });
                }
                InstData::SymbolAddr { symbol } => {
                    let name = func.symbol_name(*symbol);
                    if symbols
                        .data_object(name)
                        .is_some_and(|data| data.thread_local)
                    {
                        return None;
                    }
                    let operation = Symbolic::Address {
                        symbol: *symbol,
                        model,
                        defined: symbols.defines(name),
                    };
                    let length = operation.length();
                    sites.push(Site {
                        offset: bytes.len(),
                        operation,
                    });
                    bytes.resize(bytes.len() + length, 0);
                    store(&mut bytes, 0, home(func, base, dst?));
                }
                InstData::StackAddr { slot } => {
                    bytes.extend_from_slice(&[0x48, 0x8d, 0x84, 0x24]);
                    bytes.extend_from_slice(&slots[slot.index()].to_le_bytes());
                    store(&mut bytes, 0, home(func, base, dst?));
                }
                InstData::Call { func: callee, args } => {
                    let sig = &func.callee(*callee).sig;
                    if values.len() != sig.returns.len() {
                        return None;
                    }
                    arguments(func, base, args, sig, &mut bytes)?;
                    sites.push(Site {
                        offset: bytes.len(),
                        operation: Symbolic::Call(*callee),
                    });
                    // Publishing replaces exactly these five bytes with a symbolic CALL.
                    bytes.extend_from_slice(&[0xe8, 0, 0, 0, 0]);
                    capture_results(func, base, values, sig, &mut bytes)?;
                }
                InstData::CallIndirect { sig, callee, args } => {
                    let sig = func.signature(*sig);
                    if func.value_type(*callee) != Type::Ptr || values.len() != sig.returns.len() {
                        return None;
                    }
                    arguments(func, base, args, sig, &mut bytes)?;
                    load(&mut bytes, 10, home(func, base, *callee));
                    bytes.extend_from_slice(&[0x41, 0xff, 0xd2]);
                    capture_results(func, base, values, sig, &mut bytes)?;
                }
                InstData::Intrinsic { id, args }
                    if matches!(
                        id,
                        Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset
                    ) =>
                {
                    if args.len() != 3 || !values.is_empty() {
                        return None;
                    }
                    for ((&value, &ty), reg) in args.iter().zip(id.info().params).zip([7, 6, 2]) {
                        if func.value_type(value) != ty {
                            return None;
                        }
                        load(&mut bytes, 0, home(func, base, value));
                        bytes.extend_from_slice(&[0x48, 0x89, 0xc0 | reg]);
                    }
                    let name = match id {
                        Intrinsic::Memcpy => "memcpy",
                        Intrinsic::Memmove => "memmove",
                        _ => "memset",
                    };
                    sites.push(Site {
                        offset: bytes.len(),
                        operation: Symbolic::Memory(name),
                    });
                    bytes.extend_from_slice(&[0xe8, 0, 0, 0, 0]);
                }
                InstData::Intrinsic {
                    id: Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd,
                    ..
                } => {}
                InstData::Jump { dest } => edge(func, base, dest, &mut bytes, &mut branches),
                InstData::Brif { cond, dests } => {
                    width(func.value_type(*cond))?;
                    load(&mut bytes, 0, home(func, base, *cond));
                    bytes.extend_from_slice(&[0x48, 0x85, 0xc0, 0x0f, 0x84]);
                    let position = bytes.len();
                    bytes.extend_from_slice(&[0; 4]);
                    edge(func, base, &dests[0], &mut bytes, &mut branches);
                    let target = bytes.len();
                    displacement(&mut bytes, position, target)?;
                    edge(func, base, &dests[1], &mut bytes, &mut branches);
                }
                InstData::Switch { arg, dests, cases } => {
                    let ty = func.value_type(*arg);
                    if !integer(ty) || dests.len() != cases.len() + 1 {
                        return None;
                    }
                    let mask = u64::MAX >> (64 - width(ty)?);
                    load(&mut bytes, 0, home(func, base, *arg));
                    for (case, dest) in cases.iter().zip(&dests[1..]) {
                        bytes.extend_from_slice(&[0x48, 0xb9]);
                        bytes.extend_from_slice(&((*case as u64 & mask).to_le_bytes()));
                        bytes.extend_from_slice(&[0x48, 0x39, 0xc8, 0x0f, 0x85]);
                        let next = bytes.len();
                        bytes.extend_from_slice(&[0; 4]);
                        edge(func, base, dest, &mut bytes, &mut branches);
                        let target = bytes.len();
                        displacement(&mut bytes, next, target)?;
                    }
                    edge(func, base, &dests[0], &mut bytes, &mut branches);
                }
                InstData::Trap => bytes.extend_from_slice(&[0x0f, 0x0b]),
                InstData::Return { args } if args.len() == func.sig.returns.len() => {
                    if func.sig.returns.len() <= 1 {
                        if let Some(&arg) = args.first() {
                            load(&mut bytes, 0, home(func, base, arg));
                            if func.sig.returns[0].ext == ArgExt::Sext {
                                signed(&mut bytes, 0, width(func.value_type(arg))?);
                            }
                        }
                    } else {
                        for ((&arg, param), reg) in args
                            .iter()
                            .zip(&func.sig.returns)
                            .zip(result_registers(&func.sig)?)
                        {
                            if func.value_type(arg) != param.ty {
                                return None;
                            }
                            load(&mut bytes, reg, home(func, base, arg));
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
    if !traps.is_empty() {
        let target = bytes.len();
        bytes.extend_from_slice(&[0x0f, 0x0b]);
        for position in traps {
            displacement(&mut bytes, position, target)?;
        }
    }
    for (position, block) in branches {
        displacement(&mut bytes, position, offsets[block])?;
    }
    Some(Code {
        bytes,
        frame,
        epilogues,
        sites,
    })
}

fn body(
    func: &Function,
    symbols: &crate::symbols::Symbols<'_>,
    out: &mut Output,
    bytes: &[u8],
    position: &mut usize,
    end: usize,
    sites: &mut std::iter::Peekable<std::slice::Iter<'_, Site>>,
) {
    use keel_asm_ir::x86::{Modifier, Opcode};
    while sites.peek().is_some_and(|site| site.offset < end) {
        let site = sites.next().expect("peeked symbolic site");
        out.bytes(&bytes[*position..site.offset]);
        match site.operation {
            Symbolic::Call(callee) => {
                let name = &func.callee(callee).name;
                out.branch_to_function(Opcode::Call, name, !symbols.defines(name));
            }
            Symbolic::Memory(name) => {
                // Ordinary intrinsic lowering always uses the helper's PLT binding,
                // including when a module also defines a function with that name.
                out.branch_to_function(Opcode::Call, name, true);
            }
            Symbolic::Address {
                symbol,
                model,
                defined,
            } => {
                let name = func.symbol_name(symbol);
                match model {
                    CodeModel::Small => out.instruction(
                        Opcode::Mov,
                        [
                            asm_arg(r32(0)),
                            asm_arg(symbol_address(name, Modifier::None)),
                        ],
                        false,
                    ),
                    CodeModel::Kernel => out.instruction(
                        Opcode::Mov,
                        [
                            asm_arg(r64(0)),
                            asm_arg(symbol_address(name, Modifier::None)),
                        ],
                        false,
                    ),
                    CodeModel::PositionIndependent if defined => out.instruction(
                        Opcode::Lea,
                        [
                            asm_arg(r64(0)),
                            asm_arg(symbol_memory(rip(), name, Modifier::None)),
                        ],
                        false,
                    ),
                    CodeModel::PositionIndependent => out.instruction(
                        Opcode::Mov,
                        [
                            asm_arg(r64(0)),
                            asm_arg(sized(symbol_memory(rip(), name, Modifier::GotPcRel), 64)),
                        ],
                        false,
                    ),
                }
            }
        }
        *position = site.offset + site.operation.length();
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
    let Some(code) = encode(func, symbols, emission.x86_code_model.unwrap_or_default()) else {
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
    let mut sites = code.sites.iter().peekable();
    for epilogue in code.epilogues {
        body(
            func,
            symbols,
            out,
            &code.bytes,
            &mut position,
            epilogue + 7,
            &mut sites,
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
        &mut sites,
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
