//! C interface for constructing scalar SSA functions and emitting native objects.
//!
//! The maintained ABI and ownership contract is in `include/keel.h`.

use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};

use keel_ir::{
    BinaryOp, Block, EntityRef, Function, InsertPoint, IntCC, Module, Signature, Type, Value,
};

/// C status code. See `keel_status` in the public header.
pub type KeelStatus = u32;
const OK: KeelStatus = 0;
const INVALID: KeelStatus = 1;
const VERIFY: KeelStatus = 2;
const COMPILE: KeelStatus = 3;
const PANIC: KeelStatus = 4;
const POISONED: KeelStatus = 5;

type Error = (KeelStatus, String);
type Result<T> = std::result::Result<T, Error>;

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

fn owner() -> u64 {
    // Never reuse an owner, including after integer wraparound.
    let mut current = NEXT_OWNER.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| std::process::abort());
        match NEXT_OWNER.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => return current,
            Err(observed) => current = observed,
        }
    }
}

/// Opaque module owner. Access it only through this API.
#[derive(Debug)]
pub struct KeelModule {
    ir: Module,
    owner: u64,
    error: CString,
    poisoned: bool,
    signatures: Vec<Signature>,
    symbols: Vec<systems::SymbolEntry>,
}

/// Opaque output owner, independent of the module that produced it.
#[derive(Debug)]
pub struct KeelBuffer(Vec<u8>);

/// A function ID scoped to a module generation.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelFunction {
    pub owner: u64,
    pub index: u32,
}

/// A block ID scoped to a function and module generation.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelBlock {
    pub owner: u64,
    pub function: u32,
    pub index: u32,
}

/// A value ID scoped to a function and module generation.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelValue {
    pub owner: u64,
    pub function: u32,
    pub index: u32,
}

/// A module-owned immutable C calling-convention signature.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelSignature {
    pub owner: u64,
    pub index: u32,
}

/// A module-owned function or data symbol, including forward declarations.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelSymbol {
    pub owner: u64,
    pub index: u32,
}

/// A fixed-size stack allocation scoped to a function.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelStackSlot {
    pub owner: u64,
    pub function: u32,
    pub index: u32,
}

/// A scalar ABI parameter, including explicit narrow-integer extension.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelAbiParam {
    pub kind: u32,
    pub extension: u32,
}

/// Alignment and access flags for a non-atomic load or store.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelMemory {
    pub align: u32,
    pub flags: u32,
}

fn invalid(message: &str) -> Error {
    (INVALID, message.into())
}

fn ty(raw: u32) -> Result<Type> {
    match raw {
        1 => Ok(Type::I8),
        2 => Ok(Type::I16),
        3 => Ok(Type::I32),
        4 => Ok(Type::I64),
        5 => Ok(Type::Ptr),
        _ => Err(invalid("unknown scalar type")),
    }
}

fn signature_ty(raw: u32) -> Result<Type> {
    match ty(raw)? {
        t @ (Type::I32 | Type::I64 | Type::Ptr) => Ok(t),
        _ => Err(invalid(
            "use a signature with explicit ABI extension for i8/i16",
        )),
    }
}

impl KeelModule {
    fn function_index(&self, f: KeelFunction) -> Result<usize> {
        if f.owner != self.owner || f.index as usize >= self.ir.functions.len() {
            return Err(invalid("foreign, stale or invalid function ID"));
        }
        Ok(f.index as usize)
    }

    fn block(&self, b: KeelBlock) -> Result<Block> {
        let f = self.function_index(KeelFunction {
            owner: b.owner,
            index: b.function,
        })?;
        if b.index as usize >= self.ir.functions[f].num_blocks() {
            return Err(invalid("invalid block ID"));
        }
        Ok(Block::new(b.index as usize))
    }

    fn value(&self, function: u32, v: KeelValue) -> Result<Value> {
        if v.owner != self.owner || v.function != function {
            return Err(invalid("foreign or stale value ID"));
        }
        let f = &self.ir.functions[function as usize];
        if v.index as usize >= f.num_values() {
            return Err(invalid("invalid value ID"));
        }
        Ok(Value::new(v.index as usize))
    }

    fn value_id(&self, function: u32, value: Value) -> KeelValue {
        KeelValue {
            owner: self.owner,
            function,
            index: value.index() as u32,
        }
    }

    fn insertion(&self, b: KeelBlock) -> Result<Block> {
        let block = self.block(b)?;
        let f = &self.ir.functions[b.function as usize];
        if f.block_insts(block)
            .last()
            .is_some_and(|i| f.inst_data(*i).is_terminator())
        {
            return Err(invalid("block already has a terminator"));
        }
        Ok(block)
    }

    fn values(&self, function: u32, args: &[KeelValue]) -> Result<Vec<Value>> {
        args.iter().map(|v| self.value(function, *v)).collect()
    }

    fn destination(&self, from: KeelBlock, to: KeelBlock, args: &[Value]) -> Result<Block> {
        let dest = self.block(to)?;
        if from.function != to.function {
            return Err(invalid("branch destination belongs to another function"));
        }
        let f = &self.ir.functions[from.function as usize];
        let params = f.block_params(dest);
        if params.len() != args.len()
            || params
                .iter()
                .zip(args)
                .any(|(p, a)| f.value_type(*p) != f.value_type(*a))
        {
            return Err(invalid("branch arguments do not match block parameters"));
        }
        Ok(dest)
    }

    fn verify(&self) -> Result<()> {
        keel_ir::verify_module(&self.ir).map_err(|errors| {
            (
                VERIFY,
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        })
    }
}

/// Run one operation, converting unexpected unwinds into a poisoned module.
///
/// # Safety
/// `module` must be null or a live, exclusively accessible API module pointer.
unsafe fn run(
    module: *mut KeelModule,
    action: impl FnOnce(&mut KeelModule) -> Result<()>,
) -> KeelStatus {
    // SAFETY: The caller guarantees validity and exclusive access when nonnull.
    let Some(module) = (unsafe { module.as_mut() }) else {
        return INVALID;
    };
    if module.poisoned {
        return POISONED;
    }
    let result = catch_unwind(AssertUnwindSafe(|| action(module)));
    let (status, message) = match result {
        Ok(Ok(())) => (OK, String::new()),
        Ok(Err(error)) => error,
        Err(payload) => {
            // A panic payload can have a panicking destructor. Do not drop it
            // outside the unwind barrier; this path is already unrecoverable.
            std::mem::forget(payload);
            module.poisoned = true;
            (PANIC, "internal panic; destroy this module".into())
        }
    };
    // Error text may contain NUL from an upstream diagnostic.
    module.error = CString::new(message.replace('\0', "\\0")).expect("NUL characters were escaped");
    status
}

/// Validate a foreign slice, accepting null only for an empty slice.
///
/// # Safety
/// Nonempty input must point to aligned, initialized, readable elements in one
/// allocation, valid for the returned lifetime, without concurrent mutation.
unsafe fn input<'a, T>(ptr: *const T, len: usize) -> Result<&'a [T]> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() || len > isize::MAX as usize / std::mem::size_of::<T>() {
        return Err(invalid("null input or excessive input length"));
    }
    // SAFETY: The caller guarantees the slice allocation and alignment; length
    // and null checks above enforce the remaining slice preconditions.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// Shared entry point for operations with an output parameter.
///
/// # Safety
/// The module follows `run`'s contract. A nonnull output must be writable,
/// aligned, and must not alias the module or any input read by the action.
unsafe fn output<T>(
    module: *mut KeelModule,
    out: *mut T,
    action: impl FnOnce(&mut KeelModule) -> Result<T>,
) -> KeelStatus {
    // SAFETY: The caller provides the module and output contracts above.
    unsafe {
        run(module, |m| {
            if out.is_null() {
                return Err(invalid("null output pointer"));
            }
            let value = action(m)?;
            out.write(value);
            Ok(())
        })
    }
}

/// Returns the ABI revision of this experimental interface.
#[unsafe(no_mangle)]
pub extern "C" fn keel_abi_version() -> u32 {
    1
}

/// Creates an empty module. Destroy it with `keel_module_destroy`.
#[unsafe(no_mangle)]
pub extern "C" fn keel_module_create() -> *mut KeelModule {
    Box::into_raw(Box::new(KeelModule {
        ir: Module::default(),
        owner: owner(),
        error: CString::default(),
        poisoned: false,
        signatures: Vec::new(),
        symbols: Vec::new(),
    }))
}

/// Destroys a module, including a poisoned module; null is accepted.
///
/// # Safety
/// A nonnull pointer must be a live module returned by this API, with no
/// outstanding accesses. It must be destroyed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_module_destroy(module: *mut KeelModule) {
    if !module.is_null() {
        // SAFETY: The caller transfers the unique live allocation back to Rust.
        drop(unsafe { Box::from_raw(module) });
    }
}

/// Borrows the last diagnostic, valid until the next operation or destruction.
/// Returns null for a null module.
///
/// # Safety
/// A nonnull module must be live and cannot be mutated during this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_module_error(module: *const KeelModule) -> *const c_char {
    // SAFETY: The caller guarantees a live, readable module when nonnull.
    unsafe { module.as_ref() }.map_or(std::ptr::null(), |m| m.error.as_ptr())
}

// Each exported operation shares this pointer contract. Integers and IDs are
// validated before accessing IR; foreign allocation validity remains the caller's job.
macro_rules! api {
    ($(#[$attr:meta])* fn $name:ident($module:ident: *mut KeelModule $(, $arg:ident: $ty:ty)* $(,)?) $body:block) => {
        $(#[$attr])*
        ///
        /// # Safety
        /// The module must be null or live and exclusively accessible. Nonempty
        /// input arrays must be readable, aligned allocations of their stated
        /// lengths for this call. Nonnull outputs must be aligned and writable.
        /// Outputs must not alias inputs or API-owned storage. See `include/keel.h`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name($module: *mut KeelModule, $($arg: $ty),*) -> KeelStatus $body
    };
}

api! {
    /// Adds a named i32/i64/pointer function; copies all signature inputs.
    fn keel_module_add_function(module: *mut KeelModule, name: *const u8, name_len: usize,
        params: *const u32, param_count: usize, returns: *const u32, return_count: usize,
        out: *mut KeelFunction) {
        // SAFETY: The exported API's pointer contract covers these inputs and output.
        unsafe { output(module, out, |m| {
            let name = systems::name(input(name, name_len)?)?;
            m.unused_name(name)?;
            let params = input(params, param_count)?.iter().map(|t| signature_ty(*t)).collect::<Result<Vec<_>>>()?;
            let returns = input(returns, return_count)?.iter().map(|t| signature_ty(*t)).collect::<Result<Vec<_>>>()?;
            if returns.len() > 1 { return Err(invalid("at most one return value is supported")); }
            let index = u32::try_from(m.ir.functions.len()).map_err(|_| invalid("too many functions"))?;
            m.ir.functions.push(Function::new(name, Signature::new(&params, &returns)));
            Ok(KeelFunction { owner: m.owner, index })
        }) }
    }
}

api! {
    /// Creates a block; the first block created is the function entry.
    fn keel_block_create(module: *mut KeelModule, function: KeelFunction, out: *mut KeelBlock) {
        // SAFETY: The exported API's pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let f = m.function_index(function)?;
            let index = m.ir.functions[f].create_block().index() as u32;
            Ok(KeelBlock { owner: m.owner, function: function.index, index })
        }) }
    }
}

api! {
    /// Appends a typed block parameter, before any instructions in that block.
    fn keel_block_add_parameter(module: *mut KeelModule, block: KeelBlock, kind: u32, out: *mut KeelValue) {
        // SAFETY: The exported API's pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.block(block)?;
            let ty = ty(kind)?;
            let f = &mut m.ir.functions[block.function as usize];
            if !f.block_insts(b).is_empty() { return Err(invalid("add parameters before instructions")); }
            if f.entry_block() == Some(b) {
                let n = f.block_params(b).len();
                if f.sig.params.get(n).is_none_or(|p| p.ty != ty) {
                    return Err(invalid("entry parameter does not match function signature"));
                }
            }
            let v = f.append_block_param(b, ty);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends an integer constant, truncated to the requested width.
    fn keel_build_iconst(module: *mut KeelModule, block: KeelBlock, kind: u32, immediate: i64, out: *mut KeelValue) {
        // SAFETY: The exported API's pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let ty = ty(kind)?;
            if !ty.is_int() { return Err(invalid("integer constant requires an integer type")); }
            let v = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).iconst(ty, immediate);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends an integer binary operation on equal-width operands.
    fn keel_build_binary(module: *mut KeelModule, block: KeelBlock, op: u32, lhs: KeelValue, rhs: KeelValue, out: *mut KeelValue) {
        // SAFETY: The exported API's pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let a = m.value(block.function, lhs)?;
            let c = m.value(block.function, rhs)?;
            let op = match op {
                1 => BinaryOp::Iadd, 2 => BinaryOp::Isub, 3 => BinaryOp::Imul,
                4 => BinaryOp::Udiv, 5 => BinaryOp::Sdiv, 6 => BinaryOp::Urem, 7 => BinaryOp::Srem,
                8 => BinaryOp::And, 9 => BinaryOp::Or, 10 => BinaryOp::Xor,
                11 => BinaryOp::Shl, 12 => BinaryOp::Ushr, 13 => BinaryOp::Sshr,
                _ => return Err(invalid("unknown binary operation")),
            };
            let f = &mut m.ir.functions[block.function as usize];
            if !f.value_type(a).is_int() || f.value_type(a) != f.value_type(c) {
                return Err(invalid("binary operands must have the same integer type"));
            }
            let v = f.ins(InsertPoint::End(b)).binary(op, a, c);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends an integer or pointer comparison, producing an i8 condition.
    fn keel_build_icmp(module: *mut KeelModule, block: KeelBlock, condition: u32, lhs: KeelValue, rhs: KeelValue, out: *mut KeelValue) {
        // SAFETY: The exported API's pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let a = m.value(block.function, lhs)?;
            let c = m.value(block.function, rhs)?;
            let cond = match condition {
                1 => IntCC::Eq, 2 => IntCC::Ne, 3 => IntCC::Slt, 4 => IntCC::Sle,
                5 => IntCC::Sgt, 6 => IntCC::Sge, 7 => IntCC::Ult, 8 => IntCC::Ule,
                9 => IntCC::Ugt, 10 => IntCC::Uge,
                _ => return Err(invalid("unknown integer condition")),
            };
            let f = &mut m.ir.functions[block.function as usize];
            if f.value_type(a) != f.value_type(c) {
                return Err(invalid("comparison operands must have the same type"));
            }
            let v = f.ins(InsertPoint::End(b)).icmp(cond, a, c);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Terminates a block with an unconditional branch and block arguments.
    fn keel_build_jump(module: *mut KeelModule, block: KeelBlock, dest: KeelBlock, args: *const KeelValue, count: usize) {
        // SAFETY: The exported API's pointer contract covers module and input array.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let args = m.values(block.function, input(args, count)?)?;
            let dest = m.destination(block, dest, &args)?;
            m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).jump(dest, &args);
            Ok(())
        }) }
    }
}

api! {
    /// Terminates a block with an i8 conditional branch and block arguments.
    fn keel_build_brif(module: *mut KeelModule, block: KeelBlock, condition: KeelValue,
        then_block: KeelBlock, then_args: *const KeelValue, then_count: usize,
        else_block: KeelBlock, else_args: *const KeelValue, else_count: usize) {
        // SAFETY: The exported API's pointer contract covers module and input arrays.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let cond = m.value(block.function, condition)?;
            if m.ir.functions[block.function as usize].value_type(cond) != Type::I8 {
                return Err(invalid("branch condition must be i8"));
            }
            let ta = m.values(block.function, input(then_args, then_count)?)?;
            let ea = m.values(block.function, input(else_args, else_count)?)?;
            let tb = m.destination(block, then_block, &ta)?;
            let eb = m.destination(block, else_block, &ea)?;
            m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).brif(cond, tb, &ta, eb, &ea);
            Ok(())
        }) }
    }
}

api! {
    /// Terminates a block with a return matching its function signature.
    fn keel_build_return(module: *mut KeelModule, block: KeelBlock, args: *const KeelValue, count: usize) {
        // SAFETY: The exported API's pointer contract covers module and input array.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let args = m.values(block.function, input(args, count)?)?;
            let f = &mut m.ir.functions[block.function as usize];
            if args.len() != f.sig.returns.len()
                || args.iter().zip(&f.sig.returns).any(|(a, p)| f.value_type(*a) != p.ty)
            {
                return Err(invalid("return values do not match function signature"));
            }
            f.ins(InsertPoint::End(b)).return_(&args);
            Ok(())
        }) }
    }
}

api! {
    /// Verifies structure, types, names and dominance of the complete module.
    fn keel_module_verify(module: *mut KeelModule) {
        // SAFETY: The exported API's pointer contract covers module.
        unsafe { run(module, |m| m.verify()) }
    }
}

api! {
    /// Verifies and optimizes a module. Invalidates all IDs once optimization starts.
    fn keel_module_optimize(module: *mut KeelModule) {
        // SAFETY: The exported API's pointer contract covers module.
        unsafe { run(module, |m| {
            m.verify()?;
            m.owner = owner();
            m.symbols.clear();
            m.signatures.clear();
            keel_opt::optimize(&mut m.ir).map_err(|e| (COMPILE, e.to_string()))?;
            m.verify()
        }) }
    }
}

api! {
    /// Copies the current textual IR into an independently owned buffer.
    fn keel_module_emit_ir(module: *mut KeelModule, out: *mut *mut KeelBuffer) {
        // SAFETY: The exported API's pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            Ok(Box::into_raw(Box::new(KeelBuffer(m.ir.to_string().into_bytes()))))
        }) }
    }
}

api! {
    /// Verifies and emits a native object for an explicit target.
    fn keel_module_emit_object(module: *mut KeelModule, target: u32, out: *mut *mut KeelBuffer) {
        // SAFETY: The exported API's pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let target = match target {
                1 => keel_ir::Target::Aarch64AppleDarwin,
                2 => keel_ir::Target::Aarch64LinuxGnu,
                3 => keel_ir::Target::X86_64LinuxGnu,
                _ => return Err(invalid("unknown native target")),
            };
            m.verify()?;
            let program = keel_codegen::compile_program(&m.ir, target).map_err(|e| (COMPILE, e.to_string()))?;
            let bytes = keel_asm::assemble_program_for(target, &program).map_err(|e| (COMPILE, e.to_string()))?;
            Ok(Box::into_raw(Box::new(KeelBuffer(bytes))))
        }) }
    }
}

/// Borrows the buffer's bytes; null returns null. Bytes are not NUL-terminated.
///
/// # Safety
/// A nonnull buffer must be live and remain live while the returned bytes are read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_buffer_data(buffer: *const KeelBuffer) -> *const u8 {
    // SAFETY: The caller guarantees the nonnull buffer is live and readable.
    unsafe { buffer.as_ref() }.map_or(std::ptr::null(), |b| b.0.as_ptr())
}

/// Returns the buffer's byte length; null returns zero.
///
/// # Safety
/// A nonnull buffer must be live and readable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_buffer_size(buffer: *const KeelBuffer) -> usize {
    // SAFETY: The caller guarantees the nonnull buffer is live and readable.
    unsafe { buffer.as_ref() }.map_or(0, |b| b.0.len())
}

/// Destroys a buffer; null is accepted.
///
/// # Safety
/// A nonnull pointer must be a live buffer returned by this API, with no
/// outstanding accesses. It must be destroyed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_buffer_destroy(buffer: *mut KeelBuffer) {
    if !buffer.is_null() {
        // SAFETY: The caller transfers the unique live allocation back to Rust.
        drop(unsafe { Box::from_raw(buffer) });
    }
}

mod systems;

#[cfg(test)]
mod tests;
