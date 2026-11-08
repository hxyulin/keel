//! Typed atomic operations, keeping memory-order validation at the C boundary.
use super::{KeelBlock, KeelModule, KeelStatus, KeelValue, Result, invalid, output, run, ty};
use keel_ir::{InsertPoint, Ordering, RmwOp, Type};

fn ordering(raw: u32) -> Result<Ordering> {
    match raw {
        0 => Ok(Ordering::Relaxed),
        1 => Ok(Ordering::Acquire),
        2 => Ok(Ordering::Release),
        3 => Ok(Ordering::AcqRel),
        4 => Ok(Ordering::SeqCst),
        _ => Err(invalid("unknown memory ordering")),
    }
}
fn atomic_type(kind: Type, pointer: bool) -> Result<()> {
    if !kind.is_int() && !(pointer && kind == Type::Ptr) {
        return Err(invalid("atomic requires integer or permitted pointer type"));
    }
    Ok(())
}
api! {
    /// Appends an atomic integer/pointer load. Release and acq-rel are invalid.
    fn keel_build_atomic_load(module: *mut KeelModule, block: KeelBlock, kind: u32, order: u32, address: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?; let address = m.pointer(block.function, address)?;
            let kind = ty(kind)?; atomic_type(kind, true)?; let order = ordering(order)?;
            if matches!(order, Ordering::Release | Ordering::AcqRel) { return Err(invalid("atomic load cannot release")); }
            let v = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).atomic_load(kind, order, address);
            Ok(m.value_id(block.function, v))
        }) }
    }
}
api! {
    /// Appends an atomic store. Acquire and acq-rel are invalid.
    fn keel_build_atomic_store(module: *mut KeelModule, block: KeelBlock, order: u32, value: KeelValue, address: KeelValue) {
        // SAFETY: Public pointer contract covers module.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?; let address = m.pointer(block.function, address)?; let value = m.value(block.function, value)?;
            atomic_type(m.ir.functions[block.function as usize].value_type(value), true)?;
            let order = ordering(order)?;
            if matches!(order, Ordering::Acquire | Ordering::AcqRel) { return Err(invalid("atomic store cannot acquire")); }
            m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).atomic_store(order, value, address); Ok(())
        }) }
    }
}
api! {
    /// Appends an atomic RMW, returning the old value. Only exchange accepts pointers.
    fn keel_build_atomic_rmw(module: *mut KeelModule, block: KeelBlock, operation: u32, order: u32, address: KeelValue, value: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?; let address = m.pointer(block.function, address)?; let value = m.value(block.function, value)?;
            let op = match operation { 1 => RmwOp::Add, 2 => RmwOp::Sub, 3 => RmwOp::And, 4 => RmwOp::Or, 5 => RmwOp::Xor,
                6 => RmwOp::Xchg, 7 => RmwOp::Umin, 8 => RmwOp::Umax, 9 => RmwOp::Smin, 10 => RmwOp::Smax, _ => return Err(invalid("unknown atomic RMW operation")) };
            atomic_type(m.ir.functions[block.function as usize].value_type(value), op == RmwOp::Xchg)?;
            let order = ordering(order)?;
            let v = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).atomic_rmw(op, order, address, value);
            Ok(m.value_id(block.function, v))
        }) }
    }
}
api! {
    /// Appends strong/weak CAS; results are the old value and an i8 success flag.
    fn keel_build_atomic_cas(module: *mut KeelModule, block: KeelBlock, success: u32, failure: u32, strength: u32,
        address: KeelValue, expected: KeelValue, replacement: KeelValue, results: *mut KeelValue, result_count: usize) {
        // SAFETY: Public pointer contract covers module and a nonoverlapping two-value output.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            if result_count != 2 || results.is_null() { return Err(invalid("CAS requires two result slots")); }
            let address = m.pointer(block.function, address)?;
            let expected = m.value(block.function, expected)?; let replacement = m.value(block.function, replacement)?;
            let f = &mut m.ir.functions[block.function as usize];
            let kind = f.value_type(expected); atomic_type(kind, true)?;
            if kind != f.value_type(replacement) { return Err(invalid("CAS values must have equal type")); }
            let success = ordering(success)?; let failure = ordering(failure)?;
            if matches!(failure, Ordering::Release | Ordering::AcqRel) { return Err(invalid("CAS failure cannot release")); }
            if strength > 1 { return Err(invalid("unknown CAS strength")); }
            let builder = f.ins(InsertPoint::End(b));
            let (old, flag) = if strength == 0 { builder.atomic_cas(success, failure, address, expected, replacement) } else { builder.atomic_cas_weak(success, failure, address, expected, replacement) };
            results.write(m.value_id(block.function, old)); results.add(1).write(m.value_id(block.function, flag)); Ok(())
        }) }
    }
}
api! {
    /// Appends a memory fence; relaxed fences are invalid.
    fn keel_build_fence(module: *mut KeelModule, block: KeelBlock, order: u32) {
        // SAFETY: Public pointer contract covers module.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?; let order = ordering(order)?;
            if order == Ordering::Relaxed { return Err(invalid("fence cannot be relaxed")); }
            m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).fence(order); Ok(())
        }) }
    }
}
