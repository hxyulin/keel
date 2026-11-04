//! A reference interpreter for Keel IR, for tests.
//!
//! It runs the functions of a [`Module`] on [`Val`]s: integers, `f32`/`f64`,
//! pointers and vectors of every shape. The semantics are those of
//! `docs/compiler/design.md` and `docs/compiler/simd-ir.md`, written
//! independently of the optimizer so that tests can compare a function's
//! results before and after a pass.
//!
//! ```
//! use keel_ir_interp::{Interpreter, Outcome, Val};
//!
//! let module = keel_ir::parse(
//!     "func @add(i32, i32) -> i32 {\n\
//!      block0(v0: i32, v1: i32):\n\
//!          v2 = iadd v0, v1\n\
//!          return v2\n\
//!      }\n",
//! )
//! .unwrap();
//! let mut interp = Interpreter::new(&module);
//! let outcome = interp.call("add", &[Val::I32(2), Val::I32(40)]);
//! assert_eq!(outcome, Outcome::Return(vec![Val::I32(42)]));
//! ```
//!
//! # Model
//!
//! - Memory is a set of byte allocations in one 64-bit address space (see
//!   [`Memory`]). Each stack slot of each activation, each defined data
//!   object and each `stack_alloc` block is its own allocation. An access
//!   outside a live allocation traps; so does a store to `const` data, an
//!   access that breaks its `align` promise, and an atomic access that is not
//!   naturally aligned. Fresh memory reads as zero.
//! - Function addresses are distinct non-data addresses. `call_indirect`
//!   through any other address traps.
//! - Calls go to functions defined in the same module. A call to a declared
//!   function stops with [`Outcome::Unsupported`]. `byval` parameters receive
//!   a private copy, made at the call. `return_call` replaces the frame, so
//!   tail recursion runs in constant depth. `invoke` calls and continues to
//!   its normal destination, since no callee can throw.
//! - Float arithmetic follows IEEE 754 with round-to-nearest-even. A NaN
//!   result of arithmetic or conversion is the canonical quiet NaN; `fneg`,
//!   `fabs`, `fcopysign`, `bitcast`, loads and stores keep NaN payloads.
//! - Supported intrinsics: `memcpy`, `memmove`, `memset`, `memcmp`,
//!   `stack_alloc`, `stack_save`, `stack_restore`, `lifetime_start` and
//!   `lifetime_end`. Between `lifetime_end` and `lifetime_start` a stack
//!   slot is not live, and accesses to it trap.
//! - Not modelled, reported as [`Outcome::Unsupported`]: `f80`, NEON and
//!   other target intrinsics, inline assembly, x86 FLAGS regions, variadic
//!   arguments, landing pads and `eh_typeid`.
//! - Each call has a step limit (instructions executed) and a call-depth limit.

mod exec;
mod memory;
mod ops;
mod value;

pub use memory::Memory;
pub use value::Val;

use keel_ir::{DataItem, DataObject, Function, Module};
use std::collections::HashMap;

/// A fault that ends execution, as the program would on hardware.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Trap {
    /// `udiv`, `sdiv`, `urem` or `srem` by zero.
    DivisionByZero,
    /// `sdiv` of the minimum value by -1.
    IntegerOverflow,
    /// An access to `size` bytes at `addr` not inside one live allocation.
    OutOfBounds { addr: u64, size: u64 },
    /// An access at `addr` that breaks its `align` promise, or an atomic
    /// that is not naturally aligned.
    Misaligned { addr: u64, align: u64 },
    /// A store to a `const` data object.
    WriteToConstant { addr: u64 },
    /// A `trap` terminator.
    Unreachable,
    /// An indirect call through an address that is not a function's.
    BadCallTarget { addr: u64 },
    /// More nested calls than the depth limit.
    StackOverflow,
}

/// How a call ended.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The function returned these values.
    Return(Vec<Val>),
    Trap(Trap),
    /// The call executed more instructions than the step limit.
    StepLimit,
    /// The program used something the interpreter does not model.
    Unsupported(String),
    /// The IR broke a rule the verifier enforces, or the arguments did not
    /// match the signature.
    Invalid(String),
}

/// Why execution stopped early; [`Outcome`] without the normal return.
#[derive(Debug)]
pub(crate) enum Stop {
    Trap(Trap),
    StepLimit,
    Unsupported(String),
    Invalid(String),
}

impl Stop {
    pub(crate) fn unsupported(what: impl Into<String>) -> Self {
        Self::Unsupported(what.into())
    }
}

impl From<Trap> for Stop {
    fn from(trap: Trap) -> Self {
        Self::Trap(trap)
    }
}

/// Lowest function address; functions sit 16 bytes apart below data memory.
const FIRST_FUNCTION: u64 = 0x1000;

/// Runs the functions of one module. Memory, including data objects, persists
/// across calls, so a test can call a function and then inspect memory.
#[derive(Debug)]
pub struct Interpreter<'m> {
    functions: HashMap<&'m str, &'m Function>,
    /// Addresses of functions (defined or declared) and data objects.
    symbols: HashMap<&'m str, Result<u64, String>>,
    /// Function name by address.
    function_at: HashMap<u64, &'m str>,
    memory: Memory,
    step_limit: u64,
    depth_limit: usize,
}

impl<'m> Interpreter<'m> {
    /// An interpreter for `module`, with its data objects in memory.
    pub fn new(module: &'m Module) -> Self {
        let mut interp = Self::empty(module.functions.iter());
        for decl in &module.decls {
            interp.add_function_symbol(&decl.name);
        }
        interp.load_data(&module.data);
        interp
    }

    /// An interpreter for one function outside a module. It cannot call
    /// other functions or address data.
    pub fn for_function(func: &'m Function) -> Self {
        Self::empty(std::iter::once(func))
    }

    fn empty(functions: impl Iterator<Item = &'m Function>) -> Self {
        let mut interp = Self {
            functions: HashMap::new(),
            symbols: HashMap::new(),
            function_at: HashMap::new(),
            memory: Memory::default(),
            step_limit: 1_000_000,
            depth_limit: 1_000,
        };
        for func in functions {
            // A later definition replaces an earlier one, as in the module.
            interp.functions.insert(&func.name, func);
            interp.add_function_symbol(&func.name);
        }
        interp
    }

    /// Instructions one [`Interpreter::call`] may execute; the default is a million.
    pub fn with_step_limit(mut self, steps: u64) -> Self {
        self.step_limit = steps;
        self
    }

    /// Nested calls one [`Interpreter::call`] may make; the default is 1000.
    pub fn with_depth_limit(mut self, depth: usize) -> Self {
        self.depth_limit = depth;
        self
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// The memory, for tests to place arguments or inspect results.
    pub fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }

    /// The address of a function or defined data object.
    pub fn symbol_addr(&self, name: &str) -> Option<u64> {
        self.symbols.get(name)?.as_ref().ok().copied()
    }

    /// Calls the function `name` with `args`.
    pub fn call(&mut self, name: &str, args: &[Val]) -> Outcome {
        let Some(&func) = self.functions.get(name) else {
            return Outcome::Unsupported(format!("call to @{name}, which is not defined"));
        };
        match self.run(func, args.to_vec()) {
            Ok(values) => Outcome::Return(values),
            Err(Stop::Trap(trap)) => Outcome::Trap(trap),
            Err(Stop::StepLimit) => Outcome::StepLimit,
            Err(Stop::Unsupported(what)) => Outcome::Unsupported(what),
            Err(Stop::Invalid(why)) => Outcome::Invalid(why),
        }
    }

    fn add_function_symbol(&mut self, name: &'m str) {
        if self.symbols.contains_key(name) {
            return;
        }
        let addr = FIRST_FUNCTION + 16 * self.function_at.len() as u64;
        self.symbols.insert(name, Ok(addr));
        self.function_at.insert(addr, name);
    }

    /// Allocates every defined data object, then fills them in, so that
    /// pointer items can refer to any symbol.
    fn load_data(&mut self, data: &'m [DataObject]) {
        let mut placed = Vec::new();
        for object in data {
            let Some(init) = &object.init else {
                self.symbols
                    .entry(&object.name)
                    .or_insert_with(|| Err(format!("data @{} has no definition", object.name)));
                continue;
            };
            let size = init.size();
            let symbol = match self
                .memory
                .try_alloc(size, u64::from(init.align), init.constant)
            {
                Ok(base) => {
                    placed.push((object.name.as_str(), base, init));
                    Ok(base)
                }
                Err(_) => Err(format!("data @{} is too large", object.name)),
            };
            self.symbols.insert(&object.name, symbol);
        }
        // Objects pointing at a symbol without an address cannot be read faithfully.
        let mut unplaced = Vec::new();
        for (name, base, init) in placed {
            let mut offset = 0;
            for item in &init.items {
                let bytes = match item {
                    DataItem::Bytes(bytes) => bytes.clone(),
                    DataItem::Zero(n) => vec![0; *n as usize],
                    DataItem::Int { ty, value } => {
                        let size = ty.bytes().unwrap_or(8) as usize;
                        i128::from(*value).to_le_bytes()[..size].to_vec()
                    }
                    DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol }
                        if self.symbol_addr(symbol).is_none() =>
                    {
                        unplaced.push((name, symbol));
                        vec![0; 8]
                    }
                    DataItem::Ptr { symbol, addend } => {
                        let addr = self.symbol_addr(symbol).unwrap();
                        addr.wrapping_add(u64::from(*addend)).to_le_bytes().to_vec()
                    }
                    DataItem::RttiName { symbol } => {
                        let addr = self.symbol_addr(symbol).unwrap();
                        addr.wrapping_add(1 << 63).to_le_bytes().to_vec()
                    }
                };
                self.memory.init(base, offset, &bytes);
                offset += bytes.len() as u64;
            }
        }
        for (name, symbol) in unplaced {
            self.symbols.insert(
                name,
                Err(format!(
                    "data @{name} refers to @{symbol}, which has no address"
                )),
            );
        }
    }
}
