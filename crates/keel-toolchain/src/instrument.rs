//! Opt-in memory-access callback insertion, after ordinary optimization.
//!
//! Runtime ABI: `void __keel_check_memory_v1(void *address, uint64_t bytes,
//! uint32_t flags)`. Flags: READ=1, WRITE=2, ATOMIC=4, VOLATILE=8.
//! A runtime must be built without instrumentation, preserve program memory
//! and synchronization, ignore zero-length ranges, and stop before an invalid
//! access. This crate supplies neither a runtime nor allocation/shadow tracking.
//! Callbacks preserve the original accesses and their ordering, but add calls
//! and their observable effects. They are not ThreadSanitizer instrumentation.

use crate::{Error, validate};
use keel_ir::{
    FuncDecl, Function, FunctionAttrs, InsertPoint, Inst, InstData, Intrinsic, Module, Signature,
    Type, Value,
};

pub const HOOK: &str = "__keel_check_memory_v1";
pub const READ: u32 = 1;
pub const WRITE: u32 = 2;
pub const ATOMIC: u32 = 4;
pub const VOLATILE: u32 = 8;

#[derive(Clone, Copy, Debug)]
enum Length {
    Fixed(u32),
    Dynamic(Value),
}

#[derive(Clone, Copy, Debug)]
struct Access {
    at: Inst,
    address: Value,
    length: Length,
    flags: u32,
}

/// Coverage of explicit IR operations only. Calls may access arbitrary memory
/// in their callees. Inline asm, variadic ABI operations and NEON memory
/// intrinsics are rejected until their access descriptions are implemented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub callbacks: usize,
    pub functions: usize,
}

fn signature() -> Signature {
    Signature::new(&[Type::Ptr, Type::I64, Type::I32], &[])
}

fn accesses(function: &Function) -> Result<Vec<Access>, Error> {
    let mut result = Vec::new();
    for block in function.blocks() {
        for &at in function.block_insts(block) {
            let mut fixed = |address, ty: Type, flags| -> Result<(), Error> {
                // All current Keel targets use eight-byte pointers. Target
                // expansion must revisit this together with data::PTR_BYTES.
                let bytes = ty.bytes().unwrap_or(keel_ir::data::PTR_BYTES as u32);
                result.push(Access {
                    at,
                    address,
                    length: Length::Fixed(bytes),
                    flags,
                });
                Ok(())
            };
            match function.inst_data(at) {
                InstData::Load { addr, ty, flags } => {
                    fixed(*addr, *ty, READ | if flags.volatile { VOLATILE } else { 0 })?
                }
                InstData::Store { addr, value, flags } => fixed(
                    *addr,
                    function.value_type(*value),
                    WRITE | if flags.volatile { VOLATILE } else { 0 },
                )?,
                InstData::AtomicLoad { addr, ty, .. } => fixed(*addr, *ty, READ | ATOMIC)?,
                InstData::AtomicStore { addr, value, .. } => {
                    fixed(*addr, function.value_type(*value), WRITE | ATOMIC)?
                }
                InstData::AtomicRmw { args, .. } => {
                    fixed(args[0], function.value_type(args[1]), READ | WRITE | ATOMIC)?
                }
                InstData::AtomicCas { args, .. } => {
                    fixed(args[0], function.value_type(args[1]), READ | WRITE | ATOMIC)?
                }
                InstData::Intrinsic {
                    id:
                        id @ (Intrinsic::Memcpy
                        | Intrinsic::Memmove
                        | Intrinsic::Memset
                        | Intrinsic::Memcmp),
                    args,
                } => {
                    let first_flags = if *id == Intrinsic::Memcmp {
                        READ
                    } else {
                        WRITE
                    };
                    result.push(Access {
                        at,
                        address: args[0],
                        length: Length::Dynamic(args[2]),
                        flags: first_flags,
                    });
                    if *id != Intrinsic::Memset {
                        result.push(Access {
                            at,
                            address: args[1],
                            length: Length::Dynamic(args[2]),
                            flags: READ,
                        });
                    }
                }
                InstData::InlineAsm { .. }
                | InstData::VaArg { .. }
                | InstData::Intrinsic {
                    id: Intrinsic::VaStart,
                    ..
                } => {
                    return Err(Error(format!(
                        "{}: instrumentation does not describe assembly/variadic memory accesses",
                        function.name
                    )));
                }
                InstData::Neon { op, .. } if op.memory().is_some() => {
                    return Err(Error(format!(
                        "{}: instrumentation does not describe NEON structured memory accesses",
                        function.name
                    )));
                }
                _ => {}
            }
        }
    }
    Ok(result)
}

/// Insert one callback before each explicit access, including both ranges of
/// memory-copy/compare intrinsics. Validate and plan the entire module before
/// mutating it, so unsupported input leaves the caller's module unchanged.
/// Reject an existing hook symbol/reference to prevent accidental double
/// instrumentation or recursive instrumentation of the runtime itself.
pub fn insert_memory_checks(module: &mut Module) -> Result<Report, Error> {
    validate(module)?;
    if module.has_symbol(HOOK)
        || module
            .functions
            .iter()
            .any(|f| f.callees().any(|(_, c)| c.name == HOOK))
    {
        return Err(Error(
            "memory hook already present; runtime and instrumented units cannot be reinstrumented"
                .into(),
        ));
    }
    let plans = module
        .functions
        .iter()
        .map(accesses)
        .collect::<Result<Vec<_>, _>>()?;
    let mut report = Report::default();
    if plans.iter().all(Vec::is_empty) {
        return Ok(report);
    }
    module.decls.push(FuncDecl {
        name: HOOK.into(),
        sig: signature(),
        attrs: FunctionAttrs::default(),
    });
    for (function, plan) in module.functions.iter_mut().zip(plans) {
        if plan.is_empty() {
            continue;
        }
        report.functions += 1;
        let hook = function.import_function(HOOK, signature());
        for access in plan {
            let at = InsertPoint::Before(access.at);
            let loc = function.inst_loc(access.at);
            let length = match access.length {
                Length::Fixed(bytes) => function
                    .ins(at)
                    .loc(loc)
                    .iconst(Type::I64, i64::from(bytes)),
                Length::Dynamic(value) => value,
            };
            let flags = function
                .ins(at)
                .loc(loc)
                .iconst(Type::I32, i64::from(access.flags));
            function
                .ins(at)
                .loc(loc)
                .call(hook, &[access.address, length, flags]);
            report.callbacks += 1;
        }
    }
    debug_assert!(validate(module).is_ok());
    Ok(report)
}
