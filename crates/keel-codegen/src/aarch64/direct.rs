//! Compile-speed lowering for bounded scalar functions. Every value has an
//! eight-byte stack home. x9/x10/x11 are scratches; no liveness or allocation runs.
//! Eligibility is transactional: unsupported input leaves the output untouched
//! and uses the ordinary backend. All offsets are local to the encoded body.
use super::{Cfi, Directive, Expression, Function, Output, Sink, Type};
use keel_asm_ir::{PageKind, SymbolicInstruction};
use keel_ir::{
    ArgExt, BinaryOp, BlockCall, CallArgs, CallConv, ConvertOp, EntityRef, InstData, IntCC,
    Intrinsic, Purpose, Signature, UnaryOp, Value,
};

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
enum Site<'a> {
    Call(&'a str),
    Address { target: &'a str, kind: PageKind },
}
struct Code<'a> {
    bytes: Vec<u8>,
    frame: u32,
    /// Offsets of the ADD sp before each RET, for precise unwind transitions.
    epilogues: Vec<usize>,
    saved_lr: Option<u32>,
    sites: Vec<(usize, Site<'a>)>,
}
fn bits(ty: Type) -> Option<u32> {
    Some(match ty {
        Type::I8 => 8,
        Type::I16 => 16,
        Type::I32 => 32,
        Type::I64 | Type::Ptr => 64,
        _ => return None,
    })
}
fn normalize(out: &mut Vec<u8>, reg: u32, width: u32) {
    if width < 64 {
        word(out, 0xd3400000 | (width - 1) << 10 | reg << 5 | reg);
    }
}
fn signed(out: &mut Vec<u8>, reg: u32, width: u32) {
    if width < 64 {
        word(out, 0x93400000 | (width - 1) << 10 | reg << 5 | reg);
    }
}
fn result(out: &mut Vec<u8>, dst: u32, width: u32) {
    normalize(out, 9, width);
    store(out, 9, dst);
}
fn scalar_signature(sig: &Signature, linux: bool) -> bool {
    !sig.variadic
        && !sig.returns_twice
        && sig.params.len() <= 8
        && sig.returns.len() <= 1
        && sig
            .params
            .iter()
            .chain(&sig.returns)
            .all(|p| bits(p.ty).is_some() && p.purpose == Purpose::Normal)
        && (linux
            || sig.cc != CallConv::C
            || !sig
                .returns
                .first()
                .is_some_and(|p| matches!(p.ty, Type::I8 | Type::I16) && p.ext == ArgExt::None))
}
fn arguments(
    bytes: &mut Vec<u8>,
    func: &Function,
    sig: &Signature,
    args: &CallArgs,
    linux: bool,
) -> Option<()> {
    if !scalar_signature(sig, linux) || args.has_explicit_abi() || args.len() != sig.params.len() {
        return None;
    }
    for (index, (&value, param)) in args.iter().zip(&sig.params).enumerate() {
        if func.value_type(value) != param.ty {
            return None;
        }
        load(bytes, index as u32, home(func, value));
        if param.ext == ArgExt::Sext && matches!(param.ty, Type::I8 | Type::I16) {
            word(
                bytes,
                0x13000000 | (bits(param.ty)? - 1) << 10 | (index as u32) << 5 | index as u32,
            );
        }
    }
    Some(())
}
fn call_result(bytes: &mut Vec<u8>, sig: &Signature, dst: u32) -> Option<()> {
    if let Some(result) = sig.returns.first() {
        normalize(bytes, 0, bits(result.ty)?);
        store(bytes, 0, dst);
    }
    Some(())
}
fn materialize(out: &mut Vec<u8>, register: u32, value: u64) {
    for half in 0..4 {
        word(
            out,
            (if half == 0 { 0xd2800000 } else { 0xf2800000 })
                | register
                | half << 21
                | ((value >> (half * 16)) as u32 & 0xffff) << 5,
        );
    }
}
fn divide(out: &mut Vec<u8>, op: BinaryOp, width: u32) {
    // Hardware division returns zero on zero; the IR traps instead.
    word(out, 0xb500004a); // cbnz x10, +8
    word(out, 0xd4200020); // brk #1
    let is_signed = matches!(op, BinaryOp::Sdiv | BinaryOp::Srem);
    if is_signed {
        signed(out, 9, width);
        signed(out, 10, width);
    }
    if op == BinaryOp::Sdiv {
        word(out, 0xb100055f); // cmn x10, #1
        word(out, 0x54000101); // b.ne +32: only -1 can overflow
        materialize(out, 11, (i64::MIN >> (64 - width)) as u64);
        word(out, 0xeb0b013f); // cmp x9, x11
        word(out, 0x54000041); // b.ne +8
        word(out, 0xd4200020);
    }
    let opcode = if is_signed { 0x9aca0d20 } else { 0x9aca0920 };
    if matches!(op, BinaryOp::Urem | BinaryOp::Srem) {
        word(out, opcode | 11);
        word(out, 0x9b0aa569); // msub x9, x11, x10, x9
    } else {
        word(out, opcode | 9);
    }
}
fn encode<'a>(
    func: &'a Function,
    symbols: &crate::symbols::Symbols<'_>,
    linux: bool,
) -> Option<Code<'a>> {
    if !scalar_signature(&func.sig, linux)
        || func.personality.is_some()
        || !func.debug_vars.is_empty()
    {
        return None;
    }
    let has_calls = func.layout_insts().any(|(_, inst)| {
        matches!(
            func.inst_data(inst),
            InstData::Call { .. } | InstData::CallIndirect { .. }
        )
    });
    let extra = func
        .blocks()
        .map(|b| func.block_params(b).len())
        .max()
        .unwrap_or(0);
    let mut frame = func.num_values().checked_add(extra)?.checked_mul(8)?;
    let mut stack = vec![0; func.num_stack_slots()];
    for (slot, data) in func.stack_slots() {
        let align = data.align as usize;
        if align > 16 || !align.is_power_of_two() {
            return None;
        }
        frame = frame.checked_add(align - 1)? & !(align - 1);
        stack[slot.index()] = frame;
        frame = frame.checked_add(data.size as usize)?;
    }
    let saved_lr = if has_calls {
        frame = frame.checked_add(7)? & !7;
        let save = frame;
        frame = frame.checked_add(8)?;
        Some(save as u32)
    } else {
        None
    };
    frame = frame.checked_add(15)? & !15;
    if frame > 4080 {
        return None;
    }
    let frame = frame as u32;
    let mut bytes = Vec::with_capacity(func.num_values() * 24 + 64);
    let mut epilogues = Vec::new();
    let mut sites = Vec::new();
    word(&mut bytes, 0xd10003ff | frame << 10);
    if let Some(save) = saved_lr {
        store(&mut bytes, 30, save / 8);
    }
    let entry = func.entry_block()?;
    for (index, &value) in func.block_params(entry).iter().enumerate() {
        normalize(&mut bytes, index as u32, bits(func.value_type(value))?);
        store(&mut bytes, index as u32, home(func, value));
    }
    let mut offsets = vec![0; func.num_blocks()];
    let mut branches = Vec::new();
    for block in func.blocks() {
        offsets[block.index()] = bytes.len();
        for &value in func.block_params(block) {
            bits(func.value_type(value))?;
        }
        for &inst in func.block_insts(block) {
            let dst = func
                .inst_results(inst)
                .first()
                .map_or(0, |&v| home(func, v));
            match func.inst_data(inst) {
                InstData::Iconst { ty, imm } if ty.is_int() => {
                    let width = bits(*ty)?;
                    materialize(&mut bytes, 9, *imm as u64);
                    result(&mut bytes, dst, width);
                }
                InstData::Binary { op, args } => {
                    let ty = func.value_type(args[0]);
                    let width = bits(ty)?;
                    if *op == BinaryOp::Ptradd {
                        if ty != Type::Ptr || func.value_type(args[1]) != Type::I64 {
                            return None;
                        }
                    } else if !ty.is_int()
                        || (if matches!(op, BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr) {
                            !func.value_type(args[1]).is_int()
                                || bits(func.value_type(args[1])).is_none()
                        } else {
                            func.value_type(args[1]) != ty
                        })
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
                        BinaryOp::Sshr => 0x9aca2929,
                        BinaryOp::Udiv | BinaryOp::Sdiv | BinaryOp::Urem | BinaryOp::Srem => 0,
                        _ => return None,
                    };
                    load(&mut bytes, 9, home(func, args[0]));
                    load(&mut bytes, 10, home(func, args[1]));
                    if matches!(
                        op,
                        BinaryOp::Udiv | BinaryOp::Sdiv | BinaryOp::Urem | BinaryOp::Srem
                    ) {
                        divide(&mut bytes, *op, width);
                        result(&mut bytes, dst, width);
                        continue;
                    }
                    if matches!(op, BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr) && width < 64 {
                        // A64 masks variable X shifts to six bits; IR masks to its own width.
                        normalize(&mut bytes, 10, width.trailing_zeros());
                    }
                    if *op == BinaryOp::Sshr {
                        signed(&mut bytes, 9, width);
                    }
                    word(&mut bytes, opcode);
                    result(&mut bytes, dst, width);
                }
                InstData::Unary { op, arg } => {
                    let width = bits(func.value_type(*arg))?;
                    let opcode = match op {
                        UnaryOp::Ineg => 0xcb0903e9,
                        UnaryOp::Bnot => 0xaa2903e9,
                        _ => return None,
                    };
                    load(&mut bytes, 9, home(func, *arg));
                    word(&mut bytes, opcode);
                    result(&mut bytes, dst, width);
                }
                InstData::Convert { op, ty, arg } => {
                    let from = bits(func.value_type(*arg))?;
                    let to = bits(*ty)?;
                    if !matches!(
                        op,
                        ConvertOp::Sext
                            | ConvertOp::Uext
                            | ConvertOp::Trunc
                            | ConvertOp::PtrToInt
                            | ConvertOp::IntToPtr
                    ) {
                        return None;
                    }
                    load(&mut bytes, 9, home(func, *arg));
                    if *op == ConvertOp::Sext {
                        signed(&mut bytes, 9, from);
                    }
                    result(&mut bytes, dst, to);
                }
                InstData::Icmp { cond, args } => {
                    let width = bits(func.value_type(args[0]))?;
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
                    if matches!(cond, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge) {
                        signed(&mut bytes, 9, width);
                        signed(&mut bytes, 10, width);
                    }
                    word(&mut bytes, 0xeb0a013f);
                    word(&mut bytes, 0x9a9f07e9 | (condition ^ 1) << 12);
                    store(&mut bytes, 9, dst);
                }
                InstData::Select { args } => {
                    bits(func.value_type(args[0]))?;
                    let width = bits(func.value_type(args[1]))?;
                    load(&mut bytes, 11, home(func, args[0]));
                    load(&mut bytes, 9, home(func, args[1]));
                    load(&mut bytes, 10, home(func, args[2]));
                    word(&mut bytes, 0xf100017f);
                    word(&mut bytes, 0x9a8a1129);
                    result(&mut bytes, dst, width);
                }
                InstData::Load { ty, addr, .. } => {
                    let opcode = match bits(*ty)? {
                        8 => 0x39400129,
                        16 => 0x79400129,
                        32 => 0xb9400129,
                        64 => 0xf9400129,
                        _ => return None,
                    };
                    load(&mut bytes, 9, home(func, *addr));
                    word(&mut bytes, opcode);
                    store(&mut bytes, 9, dst);
                }
                InstData::Store { value, addr, .. } => {
                    let opcode = match bits(func.value_type(*value))? {
                        8 => 0x3900012a,
                        16 => 0x7900012a,
                        32 => 0xb900012a,
                        64 => 0xf900012a,
                        _ => return None,
                    };
                    load(&mut bytes, 9, home(func, *addr));
                    load(&mut bytes, 10, home(func, *value));
                    word(&mut bytes, opcode);
                }
                InstData::SymbolAddr { symbol } => {
                    let target = func.symbol_name(*symbol);
                    if symbols
                        .data_object(target)
                        .is_some_and(|data| data.thread_local)
                    {
                        return None;
                    }
                    let kind = if symbols.defines(target) && !symbols.exports_weak_odr(target) {
                        PageKind::Direct
                    } else {
                        PageKind::Got
                    };
                    sites.push((bytes.len(), Site::Address { target, kind }));
                    // Each symbolic instruction occupies one word in the final layout.
                    word(&mut bytes, 0x90000009);
                    word(
                        &mut bytes,
                        if kind == PageKind::Direct {
                            0x91000129
                        } else {
                            0xf9400129
                        },
                    );
                    store(&mut bytes, 9, dst);
                }
                InstData::StackAddr { slot } => {
                    word(&mut bytes, 0x910003e9 | (stack[slot.index()] as u32) << 10);
                    store(&mut bytes, 9, dst);
                }
                InstData::Intrinsic {
                    id: Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd,
                    ..
                } => {}
                InstData::Jump { dest } => edge(func, dest, &mut bytes, &mut branches),
                InstData::Switch { arg, dests, cases } => {
                    let ty = func.value_type(*arg);
                    if !ty.is_int() || dests.len() != cases.len() + 1 {
                        return None;
                    }
                    let width = bits(ty)?;
                    let mask = u64::MAX >> (64 - width);
                    load(&mut bytes, 9, home(func, *arg));
                    for (value, dest) in cases.iter().zip(&dests[1..]) {
                        materialize(&mut bytes, 10, *value as u64 & mask);
                        word(&mut bytes, 0xeb0a013f);
                        let skip = bytes.len();
                        word(&mut bytes, 0x54000001);
                        edge(func, dest, &mut bytes, &mut branches);
                        let distance = (bytes.len() - skip) / 4;
                        if distance >= 1 << 18 {
                            return None;
                        }
                        patch(&mut bytes, skip, 0x54000001 | (distance as u32) << 5);
                    }
                    edge(func, &dests[0], &mut bytes, &mut branches);
                }
                InstData::Brif { cond, dests } => {
                    bits(func.value_type(*cond))?;
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
                InstData::Call { func: callee, args } => {
                    let callee = func.callee(*callee);
                    if func.inst_results(inst).len() != callee.sig.returns.len() {
                        return None;
                    }
                    arguments(&mut bytes, func, &callee.sig, args, linux)?;
                    sites.push((bytes.len(), Site::Call(callee.name.as_str())));
                    word(&mut bytes, 0x94000000);
                    call_result(&mut bytes, &callee.sig, dst)?;
                }
                InstData::CallIndirect { sig, callee, args } => {
                    // Keep the target out of x0-x7 while staging all ABI arguments.
                    load(&mut bytes, 9, home(func, *callee));
                    let signature = func.signature(*sig);
                    if func.value_type(*callee) != Type::Ptr
                        || func.inst_results(inst).len() != signature.returns.len()
                    {
                        return None;
                    }
                    arguments(&mut bytes, func, signature, args, linux)?;
                    word(&mut bytes, 0xd63f0120);
                    call_result(&mut bytes, signature, dst)?;
                }
                InstData::Return { args } if args.len() == func.sig.returns.len() => {
                    if let Some(&value) = args.first() {
                        let ret = &func.sig.returns[0];
                        load(&mut bytes, 0, home(func, value));
                        if ret.ext == ArgExt::Sext && matches!(ret.ty, Type::I8 | Type::I16) {
                            word(&mut bytes, 0x13000000 | (bits(ret.ty)? - 1) << 10);
                        }
                    }
                    if let Some(save) = saved_lr {
                        load(&mut bytes, 30, save / 8);
                    }
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
        saved_lr,
        sites,
    })
}

pub(super) fn emit(
    func: &Function,
    symbols: &crate::symbols::Symbols<'_>,
    out: &mut Output,
    linux: bool,
    emission: crate::EmissionOptions,
) -> bool {
    // Keep the ordinary ABI rejection contract: external Darwin C calls need
    // explicit extension attributes on narrow arguments. Indirect calls can
    // target external code too, including signatures retained after IR edits.
    for (_, callee) in func.callees() {
        if super::check_call_signature(
            || format!("call to @{}", callee.name),
            &callee.sig,
            !linux && symbols.is_external(&callee.name),
        )
        .is_err()
        {
            return false;
        }
    }
    for (_, signature) in func.signatures() {
        if super::check_call_signature(|| "indirect call".into(), signature, !linux).is_err() {
            return false;
        }
    }
    let Some(code) = encode(func, symbols, linux) else {
        return false;
    };
    super::function_entry(func, out, linux, emission.function_sections);
    let unwind = emission.unwind_tables == crate::UnwindTables::All;
    if unwind {
        out.directive(Directive::Cfi(Cfi::Start));
    }
    // Merge zero-byte CFI events and symbolic BLs into the encoded layout.
    // Branch patching counted each BL placeholder as four bytes, exactly as the
    // assembler and linker will encode the symbolic call.
    let saved_lr = code.saved_lr;
    let frame = code.frame;
    let lr_rule = saved_lr.map(|save| i64::from(save) - i64::from(frame));
    let events = std::iter::once((4, Cfi::DefCfaOffset(frame.into())))
        .chain(lr_rule.map(|offset| {
            (
                8,
                Cfi::Offset {
                    register: 30,
                    offset,
                },
            )
        }))
        .chain(code.epilogues.into_iter().flat_map(move |epilogue| {
            [
                saved_lr.map(|_| (epilogue, Cfi::Restore(30))),
                Some((epilogue + 4, Cfi::DefCfaOffset(0))),
                Some((epilogue + 8, Cfi::DefCfaOffset(frame.into()))),
                lr_rule.map(|offset| {
                    (
                        epilogue + 8,
                        Cfi::Offset {
                            register: 30,
                            offset,
                        },
                    )
                }),
            ]
            .into_iter()
            .flatten()
        }));
    let mut events = events.peekable();
    let mut sites = code.sites.into_iter().peekable();
    let mut position = 0;
    loop {
        let boundary = match (events.peek(), sites.peek()) {
            (Some((a, _)), Some((b, _))) => *a.min(b),
            (Some((a, _)), None) => *a,
            (None, Some((b, _))) => *b,
            (None, None) => break,
        };
        out.bytes(&code.bytes[position..boundary]);
        position = boundary;
        while events.peek().is_some_and(|(at, _)| *at == boundary) {
            let (_, event) = events.next().unwrap();
            if unwind {
                out.directive(Directive::Cfi(event));
            }
        }
        if sites.peek().is_some_and(|(at, _)| *at == boundary) {
            let (_, site) = sites.next().unwrap();
            match site {
                Site::Call(callee) => {
                    out.branch_name(
                        true,
                        if linux {
                            super::Name::Text(callee)
                        } else {
                            super::Name::Symbol(callee)
                        },
                    );
                    position += 4;
                }
                Site::Address { target, kind } => {
                    let target = if linux {
                        target.to_owned()
                    } else {
                        format!("_{target}")
                    };
                    out.symbolic(SymbolicInstruction::PageAddress {
                        register: 9,
                        kind,
                        target: target.clone(),
                    });
                    out.symbolic(SymbolicInstruction::PageOffset {
                        register: 9,
                        base: 9,
                        load: kind != PageKind::Direct,
                        kind,
                        target,
                    });
                    position += 8;
                }
            }
        }
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
