//! IR → typed x86-64 Linux (System V, ELF) assembly or Intel source, with
//! values in registers chosen by the linear-scan allocator in
//! [`crate::regalloc`].
//!
//! The output is for the GNU assembler (`.intel_syntax noprefix`):
//! destination first, as in the Intel manuals and keel's arm64 output.
//! The legacy baseline uses SSE4.1 `roundsd` and `popcnt`; explicit v1 uses
//! software fallbacks. CPU feature legality is independent of tuning tiers.
//!
//! # Registers
//!
//! ```text
//! rax, rcx, rdx   scratch: S0, S1, S2. Also the fixed registers of the
//!                 instructions that need them: rdx:rax for mul and div,
//!                 cl for shift counts, rax for results, al for variadic
//!                 calls, rcx and rdx as the 4th and 3rd arguments
//! rbx, r12–r15    allocatable, callee-saved
//! rsi, rdi, r8, r9  allocatable, caller-saved; also arguments
//! r10             the target of indirect calls, never allocated
//! r11             scratch: breaks cycles in parallel copies
//! rbp, rsp        frame pointer and stack pointer
//!
//! xmm0–xmm12      allocatable, caller-saved (SysV has no callee-saved
//!                 vector registers); xmm0–xmm7 are also arguments
//! xmm13–xmm15     scratch
//! ```
//!
//! With only 16 general registers, keel's arm64 rule of never allocating
//! argument registers would leave five. Here four of them are allocatable
//! and a call's arguments move into place as one parallel copy, which
//! handles a value that is itself in an argument register (`f(b, a)` with
//! `a` in rdi and `b` in rsi is a swap). A value live across a call is
//! never in one, since they are caller-saved.
//!
//! **Two-address instructions.** `add rdi, rsi` overwrites its first
//! operand, so `d = a + b` is `mov d, a; add d, b`. A result may share a
//! register with an operand that dies at the same instruction (see the
//! regalloc docs on program points): if `d` is `b`'s register, the copy
//! would overwrite `b` before it is read. Commutative integer operations
//! instead use `op d, a`; subtraction runs in rax and moves the result to
//! `d` after. **A lowering must read every operand
//! before it writes the destination.**
//!
//! Values narrower than 64 bits have unspecified upper bits. Arithmetic on
//! them uses the 32-bit registers (`add eax, ecx`), whose low bits are
//! right whatever is above; compares and stores use the exact width
//! (`cmp dil, sil`); division and right shifts extend first.
//!
//! # Frame
//!
//! ```text
//! rbp + 16          incoming stack arguments, in the caller's frame
//! rbp + 8           return address
//! rbp               saved rbp
//! rbp - 8 * k       the callee-saved registers this function uses
//! ...               IR stack slots, spill slots
//! rsp               outgoing stack arguments, for the call needing most
//! ```
//!
//! The fixed frame is addressed from rbp, so it stays reachable when `rsp`
//! moves for a dynamic allocation; only the outgoing arguments are at
//! `rsp`, where calls need them. `rsp` is 16-byte aligned at every call.
//!
//! # Symbols
//!
//! Linux executables are position independent (PIE) by default: code may
//! not contain absolute addresses. A symbol defined in this module is
//! `rip`-relative (`lea rax, [rip + counter]`); one defined elsewhere (in
//! libc) comes from the global offset table (`[rip + stdout@GOTPCREL]`),
//! which the loader fills in, and calls to it go through the procedure
//! linkage table (`call puts@PLT`).

mod aes;
mod aggregate_boundary;
mod aggregate_va_arg;
mod byte_shuffle;
mod cache_stream;
mod constant_division;
mod cpu_control;
mod cx16;
mod descriptor_queries;
mod f80;
mod flags_state;
mod inline_asm;
mod intrinsic_family;
pub mod iretq;
mod legacy_x87_images;
pub mod linux_syscall;
mod loop_align;
mod lower_control;
mod lower_memory;
mod lower_scalar;
mod lower_vector;
mod owned_x87_stack;
mod packed_pair;
mod packed_search;
mod packed_shift;
mod pclmul;
mod plan;
mod port_io;
mod raw_div;
mod raw_rflags;
mod rflags_region;
mod selector_queries;
mod sha256;
mod sign_blend;
mod string_port_io;
mod system_lowering;
mod x87_basic;
mod x87_bcd;
mod x87_binary;
mod x87_compare;
mod x87_constants;
mod x87_fi_arithmetic;
mod x87_integer;
mod x87_numerical;
mod x87_region;
mod x87_state;
mod xstate;

use crate::dwarf::{self, FrameDebug};
use crate::fold::{self, iconst};
use crate::integer_division::{reducible_divisor, signed_reciprocal, unsigned_reciprocal};
use crate::lowering::{self, EdgeCopies, SwitchPlan};
use crate::metadata::{Cfi, Directive, Expression, Sink};
use crate::parallel_copy::for_each_move;
use crate::regalloc::{self, Allocation, Loc, Pools};
use crate::sysv::{self, ArgKind, ArgLoc, RAX, RCX, RDI, RDX, Slot};
use crate::{CodegenError, Format, data, loc_reg, pieces};
use keel_asm_ir::x86::{
    EvexMnemonic, EvexOpcode, Memory, Modifier, Opcode, Operand as AsmOperand, Register,
    RegisterClass, op,
};
use keel_ir::fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};
use keel_ir::{
    AbiParam, ArgExt, BinaryOp, Block, BlockCall, CallConv, ConvertOp, DebugScope, EntityRef,
    FloatCC, Function, Inst, InstData, IntCC, Intrinsic, Layout, Module, Ordering, OverflowOp,
    Purpose, RmwOp, SourceFile, SourceLoc, Type, UnaryOp, Value, ValueDef,
};
use std::fmt::Write;
mod avx256;
mod avx512;
mod avx512_bw;
mod avx512_bw_convert;
mod avx512_bw_mask;
mod avx512_bw_mask_alu;
mod avx512_bw_misc;
mod avx512_bw_mul;
mod avx512_bw_pack;
mod avx512_bw_permute;
mod avx512_bw_shift;
mod avx512_convert;
mod avx512_dq_bridge;
mod avx512_dq_convert;
mod avx512_dq_lane;
mod avx512_dq_mask_alu;
mod avx512_dq_special;
mod avx512_float;
mod avx512_fma;
mod avx512_indexed;
mod avx512_integer_convert;
mod avx512_integer_intrinsics;
mod avx512_lane;
mod avx512_mask;
mod avx512_mask_alu;
mod avx512_pack;
mod avx512_packed;
mod avx512_permute;
mod avx512_reduce;
mod avx512_scalar;
mod avx512_scalar_convert;
mod avx512_scalar_fma;
mod avx512_scalar_move;
mod avx512_special;
mod avx512_vl_convert;
mod avx512_vl_integer;
mod avx512_vl_lane;
mod avx512_vl_narrow;
mod avx512_vl_shift;
mod avx512_vl_widen;
mod gather;
mod mmx_value;
mod mxcsr_access;
mod numeric;
mod output;
mod packed_load;
mod packed_positive;
mod packed_sign;
mod raw_fma;
mod strings;
mod vector;
pub(crate) use output::Output;
use output::*;

/// The registers values are allocated to.
pub const REGISTERS: Pools = Pools {
    caller_saved: &[6, 7, 8, 9],
    callee_saved: &[3, 12, 13, 14, 15],
    float_caller_saved: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
    float_callee_saved: &[],
};

/// Scratch registers, never allocated. Spilled operands are reloaded into
/// them, a spilled result is computed in `S0`.
const S0: u8 = RAX;
const S1: u8 = RCX;
const S2: u8 = RDX;
/// Breaks cycles in parallel copies.
const S3: u8 = 11;
/// Holds the target of an indirect call while the arguments move.
const R10: u8 = 10;

/// DWARF's number for rbp, the frame base debuggers find variables from.
const DWARF_RBP: u8 = 6;

/// Vector scratch registers.
const FS0: u8 = 13;
const FS1: u8 = 14;
const FS2: u8 = 15;

/// Compiles a module into one assembly file for `as`/`gcc`. Declared
/// functions are left as undefined symbols for the linker to resolve.
pub fn compile(module: &Module) -> Result<String, CodegenError> {
    compile_with(module, &REGISTERS)
}

/// Like [`compile`], allocating only from `pools`, which must be drawn from
/// [`REGISTERS`]. Tests use a tiny set to force spills.
pub fn compile_with(module: &Module, pools: &Pools) -> Result<String, CodegenError> {
    let selector = crate::OptionsSelector(crate::SelectionOptions::default());
    compile_with_pipeline(
        module,
        pools,
        &crate::CodegenPipeline {
            selector: &selector,
            allocator: &regalloc::AllocationTier::Full,
        },
    )
}

/// Address materialization for an x86-64 ELF image. Independent of CPU features.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodeModel {
    /// RIP-relative definitions and GOT-bound external addresses for userspace.
    #[default]
    PositionIndependent,
    /// Static low-address image. Symbol addresses use zero-extended 32-bit relocations.
    /// Code and directly referenced symbols must obey small-model address/reach limits.
    Small,
    /// Static image in the negative 2 GiB. Symbol addresses use sign-extended
    /// 32-bit relocations; direct calls still require signed 32-bit reach.
    /// Does not establish a no-SIMD, unwind, red-zone or interrupt-safe ABI.
    Kernel,
}

fn request<'a>(pools: &'a Pools<'a>, pipeline: &crate::CodegenPipeline<'a>) -> crate::Request<'a> {
    crate::Request::new(crate::Target::X86_64LinuxGnu, *pipeline).pools(pools)
}

/// Compile using independently replaceable selection and allocation stages.
pub fn compile_with_pipeline(
    module: &Module,
    pools: &Pools,
    pipeline: &crate::CodegenPipeline<'_>,
) -> Result<String, CodegenError> {
    request(pools, pipeline).text(module)
}

pub fn compile_program_with_pipeline(
    module: &Module,
    pools: &Pools,
    pipeline: &crate::CodegenPipeline<'_>,
) -> Result<keel_asm_ir::Program, CodegenError> {
    request(pools, pipeline).program(module)
}

/// Hand the program to `sink` in order, at least once per function.
pub fn compile_program_chunks_with_pipeline(
    module: &Module,
    pools: &Pools,
    pipeline: &crate::CodegenPipeline<'_>,
    sink: &mut dyn FnMut(keel_asm_ir::Program),
) -> Result<(), CodegenError> {
    request(pools, pipeline).program_chunks(module, sink)
}

pub(crate) fn compile_output<'module>(
    module: &'module Module,
    pools: &'module Pools<'module>,
    pipeline: &crate::CodegenPipeline<'_>,
    emission: crate::EmissionOptions,
    output: crate::emission::OutputOptions<'_, 'module>,
    mut sink: crate::emission::Sink<'_>,
) -> Result<Output, CodegenError> {
    let crate::emission::OutputOptions {
        structured,
        parallel,
    } = output;
    crate::validate_function_entries(module)?;
    let selector = crate::emission::FunctionSelector(pipeline.selector);
    let pipeline = &crate::CodegenPipeline {
        selector: &selector,
        allocator: pipeline.allocator,
    };
    let code_model = emission.x86_code_model.unwrap_or_default();
    if code_model == CodeModel::Kernel && module.data.iter().any(|d| d.thread_local) {
        return Err(CodegenError {
            func: String::new(),
            message: "kernel code model does not support userspace TLS".into(),
        });
    }
    keel_ir::section::validate_module(module, crate::Target::X86_64LinuxGnu).map_err(
        |message| CodegenError {
            func: String::new(),
            message,
        },
    )?;
    crate::validate_odr(module, crate::Target::X86_64LinuxGnu)?;
    validate_vector_abi(module)?;
    f80::validate(module)?;
    crate::validate_vector_width_per_function(
        module,
        8,
        |name, attrs| {
            crate::x86_features::function_features(name, attrs, pipeline.selector.x86_features())
                .map(|features| {
                    if features.avx512f {
                        64
                    } else if features.avx {
                        32
                    } else {
                        16
                    }
                })
        },
        |producer, layout, unnamed| {
            producer.and_then(|p| sysv::producer_aggregate(layout,p)).is_some_and(|policy| {
            matches!(policy, sysv::ProducerAggregate::Memory)
                || (unnamed && matches!(policy, sysv::ProducerAggregate::Vector(ty) if ty.bytes().is_some_and(|n| n>=32)))
        })
        },
    )?;

    pipeline.validate_target(crate::Target::X86_64LinuxGnu)?;
    let subset = |pool: &[u8], of: &[u8]| pool.iter().all(|r| of.contains(r));
    assert!(
        subset(pools.caller_saved, REGISTERS.caller_saved)
            && subset(pools.callee_saved, REGISTERS.callee_saved)
            && subset(pools.float_caller_saved, REGISTERS.float_caller_saved)
            && subset(pools.float_callee_saved, REGISTERS.float_callee_saved),
        "register pools must be subsets of REGISTERS"
    );
    let mut out = Output::new(structured);
    if !structured {
        out.write_str("\t.intel_syntax noprefix\n").unwrap();
    }
    let debug = !module.files.is_empty() && !module.functions.is_empty();
    if debug {
        for (i, path) in module.files.iter().enumerate() {
            out.directive(Directive::File {
                index: (i + 1) as u32,
                path: path.as_bytes().to_vec(),
            });
        }
    }
    let mut source_validator = keel_asm::x86::SourceValidator::new();
    let mut validated_items = 0;
    out.module_assembly(&module.assembly, pipeline.selector.x86_features());
    out.validate_sources(&mut source_validator, &mut validated_items)?;
    let mut frames = Vec::new();
    let symbols = crate::symbols::Symbols::new(module);
    if parallel.is_none() {
        for original in &module.functions {
            let _function_profile = crate::backend_profile::function(&original.name);
            let features = crate::x86_features::function_features(
                &original.name,
                &original.attrs,
                pipeline.selector.x86_features(),
            )?;
            let selector = crate::x86_features::FeatureSelector {
                inner: pipeline.selector,
                features,
            };
            let local_pipeline = crate::CodegenPipeline {
                selector: &selector,
                allocator: pipeline.allocator,
            };
            let boundary = (!debug)
                .then(|| aggregate_boundary::lower(original))
                .flatten();
            let lowered = vector::lower(boundary.as_ref().unwrap_or(original), features);
            let func = crate::schedule::prepare(&lowered, emission.pressure_schedule);
            let mut compiler = FunctionCompiler::new(
                &func,
                &symbols,
                pools,
                &local_pipeline,
                structured,
                crate::emission::function_goal(&func.attrs, emission.goal),
                emission.separate_slots,
            )?;
            compiler.debug = debug;
            compiler.code_model = code_model;
            compiler.unwind_tables = emission.unwind_tables;
            compiler.function_sections = emission.function_sections;
            let slots = if debug {
                compiler.slot_frame_offsets()
            } else {
                Vec::new()
            };
            let frame_register = compiler.frame_register;
            let scope_marks = compiler.compile(&mut out)?;
            out.validate_sources(&mut source_validator, &mut validated_items)?;
            frames.push(FrameDebug {
                slots,
                scope_marks,
                frame_register,
            });
            if out.flush(&mut sink) {
                validated_items = 0;
            }
        }
    } else if let Some(parallel) = parallel {
        let symbols = std::sync::Arc::new(symbols);
        let compile = move |index: usize, worker_pipeline: crate::CodegenPipeline<'_>| {
            let mut out = Output::new(structured);
            let original = &module.functions[index];
            let selector = crate::emission::FunctionSelector(worker_pipeline.selector);
            let pipeline = &crate::CodegenPipeline {
                selector: &selector,
                allocator: worker_pipeline.allocator,
            };
            let _function_profile = crate::backend_profile::function(&original.name);
            let features = crate::x86_features::function_features(
                &original.name,
                &original.attrs,
                pipeline.selector.x86_features(),
            )?;
            let selector = crate::x86_features::FeatureSelector {
                inner: pipeline.selector,
                features,
            };
            let local_pipeline = crate::CodegenPipeline {
                selector: &selector,
                allocator: pipeline.allocator,
            };
            let boundary = (!debug)
                .then(|| aggregate_boundary::lower(original))
                .flatten();
            let lowered = vector::lower(boundary.as_ref().unwrap_or(original), features);
            let func = crate::schedule::prepare(&lowered, emission.pressure_schedule);
            let mut compiler = FunctionCompiler::new(
                &func,
                &symbols,
                pools,
                &local_pipeline,
                structured,
                crate::emission::function_goal(&func.attrs, emission.goal),
                emission.separate_slots,
            )?;
            compiler.debug = debug;
            compiler.code_model = code_model;
            compiler.unwind_tables = emission.unwind_tables;
            compiler.function_sections = emission.function_sections;
            let slots = if debug {
                compiler.slot_frame_offsets()
            } else {
                Vec::new()
            };
            let frame_register = compiler.frame_register;
            let scope_marks = compiler.compile(&mut out)?;
            Ok((
                out,
                FrameDebug {
                    slots,
                    scope_marks,
                    frame_register,
                },
            ))
        };
        crate::parallel::ordered(parallel, module.functions.len(), compile, |_, result| {
            let (body, frame) = result?;
            out.append(body);
            out.validate_sources(&mut source_validator, &mut validated_items)?;
            frames.push(frame);
            if out.flush(&mut sink) {
                validated_items = 0;
            }
            Ok(())
        })?;
    }
    for f in &module.functions {
        for (enabled, section) in [
            (f.attrs.constructor, ".init_array"),
            (f.attrs.destructor, ".fini_array"),
        ] {
            if enabled {
                out.directive(Directive::ElfLifecycleArray {
                    fini: section == ".fini_array",
                });
                out.align(3);
                out.integer(8, crate::metadata::Expression::symbol(&f.name));
            }
        }
    }
    for d in &module.decls {
        if d.attrs.weak {
            out.directive(Directive::ElfWeak(d.name.clone()));
        }
    }
    for d in &module.data {
        if d.weak && d.init.is_none() {
            out.directive(Directive::ElfWeak(d.name.clone()));
        }
    }
    for data in &module.data {
        if let Some(init) = &data.init {
            data::emit_elf(&mut out, data, init, emission.data_sections);
        }
    }
    if debug {
        dwarf::emit_debug_info(
            &mut out,
            Format::Elf,
            DWARF_RBP,
            module,
            &frames,
            emission.function_sections,
        );
    }
    // No executable stack.
    out.directive(Directive::ElfSection {
        name: ".note.GNU-stack".into(),
        flags: 0,
        nobits: false,
    });
    source_validator.finish().map_err(|e| CodegenError {
        func: "<module asm>".into(),
        message: e.to_string(),
    })?;
    Ok(out)
}

// Vector scalars and single-vector aggregate wrappers share one physical SIMD
// register. Larger mixed vector aggregates use the MEMORY convention.
fn validate_vector_abi(module: &Module) -> Result<(), CodegenError> {
    fn contains(l: &Layout) -> bool {
        match l {
            Layout::Scalar(t) => t.is_vector(),
            Layout::Struct(fields) => fields.iter().any(contains),
            Layout::Array(elem, _) => contains(elem),
        }
    }
    const F64X1_ABI: &str = "f64x1 SysV C ABI is unsupported: observed GCC and Clang use stack arguments, with GCC hidden-sret versus Clang XMM scalar returns; use pointer/scalar f64 boundaries or private cc(fast)";
    let f64x1 = Type::Vector(keel_ir::VectorType::F64x1);
    fn contains_zmm(layout: &Layout) -> bool {
        match layout {
            Layout::Scalar(t) => t.vector().is_some_and(|shape| shape.bytes() == 64),
            Layout::Struct(fields) => fields.iter().any(contains_zmm),
            Layout::Array(element, _) => contains_zmm(element),
        }
    }
    fn contains_f64x1(layout: &Layout) -> bool {
        match layout {
            Layout::Scalar(ty) => *ty == Type::Vector(keel_ir::VectorType::F64x1),
            Layout::Struct(fields) => fields.iter().any(contains_f64x1),
            Layout::Array(element, _) => contains_f64x1(element),
        }
    }
    let check = |name: &str, sig: &keel_ir::Signature, extra: &[&AbiParam]| {
        if sig.returns.len() > 1 {
            sysv::multi_result_locs(sig).map_err(|message| CodegenError {
                func: name.into(),
                message,
            })?;
        }
        let params = || {
            sig.params
                .iter()
                .chain(&sig.returns)
                .chain(extra.iter().copied())
        };
        let mut aggregate_bytes = 0u64;
        for layout in params().filter_map(|p| p.layout()) {
            let Some(size) = layout.checked_size() else {
                return Err(CodegenError {
                    func: name.into(),
                    message: "aggregate ABI layout size overflows u64".into(),
                });
            };
            aggregate_bytes = aggregate_bytes
                .checked_add(size)
                .filter(|n| *n <= sysv::MAX_AGGREGATE_ABI_BYTES)
                .ok_or_else(|| CodegenError {
                    func: name.into(),
                    message: "aggregate ABI contract exceeds backend frame range".into(),
                })?;
        }

        if sig.x86_vector_abi.is_some() && sig.cc != CallConv::C {
            return Err(CodegenError {
                func: name.into(),
                message: "x86_vector_abi requires cc(c), excluding private Fast".into(),
            });
        }
        // Untagged C f64x1 remains ambiguous. Tagged natural aggregate
        // layouts must satisfy the independently established producer policy.
        if sig.cc == CallConv::C
            && params().any(|p| {
                (p.ty == f64x1 && sig.x86_vector_abi.is_none())
                    || p.layout().is_some_and(|layout| {
                        contains_f64x1(layout)
                            && (sig.x86_vector_abi.is_none()
                                || sig
                                    .x86_vector_abi
                                    .and_then(|p| sysv::producer_aggregate(layout, p))
                                    .is_none())
                    })
            })
        {
            return Err(CodegenError {
                func: name.into(),
                message: F64X1_ABI.into(),
            });
        }
        if params().any(|p| {
            p.layout().is_some_and(|l| {
                sig.x86_vector_abi
                    .and_then(|p| sysv::producer_aggregate(l, p))
                    .is_none()
                    && sysv::ambiguous_vector_wrapper(l)
            })
        }) {
            return Err(CodegenError {func:name.into(),message:"wide vector wrapper with empty aggregate members has compiler-dependent SysV ABI; use x86_vector_abi or a true single-member wrapper".into()});
        }
        // Independent 125-layout GCC/Clang/expected-register primary probes
        // establish recursive natural D8 fixed and named-variadic boundaries.
        // Unnamed operands carry the same classifier contract on CallArgs.
        if params().any(|p| {
            p.layout().is_some_and(|l| {
                l.size() <= 16
                    && contains(l)
                    && sysv::vector_wrapper(l).is_none()
                    && !(sig.cc == CallConv::C
                        && (sysv::natural_d8_aggregate(l)
                            || sig
                                .x86_vector_abi
                                .and_then(|p| sysv::producer_aggregate(l, p))
                                .is_some()))
            })
        }) {
            Err(CodegenError {func:name.into(),message:"SIMD vector fields in byval/sret aggregates require SSEUP ABI classification and are not supported yet".into()})
        } else {
            Ok(())
        }
    };
    for d in &module.decls {
        check(&d.name, &d.sig, &[])?;
    }
    for f in &module.functions {
        check(&f.name, &f.sig, &[])?;
        for (_, d) in f.callees() {
            check(&f.name, &d.sig, &[])?;
        }
        for (_, s) in f.signatures() {
            check(&f.name, s, &[])?;
        }
        for (_, inst) in f.layout_insts() {
            let call = match f.inst_data(inst) {
                InstData::Call { func, args }
                | InstData::ReturnCall { func, args }
                | InstData::Invoke { func, args, .. } => Some((&f.callee(*func).sig, args)),
                InstData::CallIndirect { sig, args, .. }
                | InstData::ReturnCallIndirect { sig, args, .. } => Some((f.signature(*sig), args)),
                InstData::VaArg {
                    ty, x86_vector_abi, ..
                } => {
                    if *ty == f64x1 {
                        if !x86_vector_abi.is_some_and(|abi| f.has_x86_va_arg_provenance(inst, abi))
                        {
                            return Err(CodegenError { func: f.name.clone(), message: "f64x1 va_arg requires explicit matching local variadic provenance".into() });
                        }
                    } else if x86_vector_abi.is_some() {
                        return Err(CodegenError {
                            func: f.name.clone(),
                            message: "x86_vector_abi VaArg metadata requires f64x1".into(),
                        });
                    }
                    None
                }
                _ => None,
            };
            if let Some((sig, args)) = call {
                let extra: Vec<_> = (0..args.len()).filter_map(|i| args.abi(i)).collect();
                check(&f.name, sig, &extra)?;
            }
            if let InstData::AggregateVaArg {
                layout,
                x86_vector_abi,
                ..
            } = f.inst_data(inst)
            {
                let param = AbiParam {
                    ty: Type::Ptr,
                    ext: ArgExt::None,
                    purpose: Purpose::ByVal(layout.clone()),
                };
                check(&f.name, &f.sig, &[&param])?;
                if contains_f64x1(layout) || (x86_vector_abi.is_some() && contains_zmm(layout)) {
                    if !x86_vector_abi.is_some_and(|p| {
                        sysv::producer_aggregate(layout, p).is_some()
                            && f.has_x86_va_arg_provenance(inst, p)
                    }) {
                        return Err(CodegenError { func: f.name.clone(), message: "producer-sensitive aggregate va_arg requires explicit matching current-signature local provenance".into() });
                    }
                } else if x86_vector_abi.is_some() {
                    return Err(CodegenError {
                        func: f.name.clone(),
                        message:
                            "aggregate producer metadata requires an f64x1 or 512-bit vector layout"
                                .into(),
                    });
                }
            }
            if let Some((sig, args)) = call
                && sig.cc == CallConv::C
                && sig.x86_vector_abi.is_none()
                && args
                    .iter()
                    .skip(sig.params.len())
                    .any(|&v| f.value_type(f.resolve(v)) == f64x1)
            {
                return Err(CodegenError {
                    func: f.name.clone(),
                    message: F64X1_ABI.into(),
                });
            }
        }
    }
    Ok(())
}

/// What this backend does not handle yet, if `inst` needs it.
fn unsupported(func: &Function, inst: Inst) -> Option<&'static str> {
    let data = func.inst_data(inst);
    let wide = |v: Value| func.value_type(func.resolve(v)) == Type::I128;
    Some(match data {
        InstData::Binary {
            op: BinaryOp::Umulhi | BinaryOp::Smulhi,
            args,
        } if wide(args[0]) => "i128 umulhi/smulhi",
        InstData::Overflow {
            op: OverflowOp::Umul | OverflowOp::Smul,
            args,
        } if wide(args[0]) => "i128 overflow multiplication",
        InstData::Convert { ty, arg, .. } if (*ty == Type::I128) != wide(*arg) => match data {
            InstData::Convert {
                op: ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc,
                ..
            } => return None,
            _ => "conversions between i128 and floats",
        },
        InstData::Switch { arg, .. } if wide(*arg) => "switch on i128",
        InstData::Invoke { .. } | InstData::LandingPad { .. } | InstData::EhTypeid { .. } => {
            "exceptions"
        }
        InstData::Neon { .. } => "ARM NEON architectural intrinsic",
        _ => return None,
    })
}

fn native_fma_legal(ty: Type, features: keel_asm_ir::x86::Features) -> bool {
    ty != Type::F80
        && if ty.vector().is_some_and(|s| s.bytes() == 64) {
            features.avx512f
        } else {
            features.avx && features.fma
        }
}

/// Instructions that clobber the caller-saved registers: the shared
/// [`lowering::is_call`] set, FMA without native support, which calls libm,
/// and the x86 intrinsics and inline assembly that act as calls.
fn is_call(func: &Function, data: &InstData, features: keel_asm_ir::x86::Features) -> bool {
    match data {
        // The reserved-stack dependency permits balanced template calls.
        // Treat those calls as ABI barriers even when a template's explicit
        // clobbers name only the GPRs used by a reduced kernel call sequence.
        InstData::InlineAsm { asm, .. } => asm.inputs.contains(&keel_ir::AsmOperand::StackPointer),
        InstData::Fma { args } => {
            !native_fma_legal(func.value_type(func.resolve(args[0])), features)
        }
        InstData::Intrinsic {
            id: Intrinsic::X86AvxVzeroall | Intrinsic::X86Xrstor64 | Intrinsic::X86Fxrstor64,
            ..
        } => true,
        _ => lowering::is_call(func, data),
    }
}

/// The constant operand of `data` encoded in the instruction itself, and
/// the immediate to write. x86 immediates are 32 bits, sign-extended to 64
/// for 64-bit operations; a narrower operation only uses the low bits.
fn folded_imm(func: &Function, data: &InstData) -> Option<(Value, i64)> {
    let ty_of = |v: Value| func.value_type(func.resolve(v));
    // The immediate for an operation of type `ty`, if `v` is one.
    let imm = |v: Value, ty: Type| {
        let k = iconst(func, v)?;
        match ty {
            Type::I64 | Type::Ptr => i32::try_from(k).is_ok().then_some(k),
            Type::I128 => None,
            _ => Some(k & mask(ty) as i64),
        }
    };
    match *data {
        InstData::Binary { op, args } => {
            let ty = ty_of(args[0]);
            match op {
                BinaryOp::Iadd
                | BinaryOp::Ptradd
                | BinaryOp::Isub
                | BinaryOp::Imul
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Xor => imm(args[1], ty).map(|k| (args[1], k)),
                BinaryOp::Shl
                | BinaryOp::Ushr
                | BinaryOp::Sshr
                | BinaryOp::Rotl
                | BinaryOp::Rotr
                    if ty != Type::I128 =>
                {
                    let bits = 8 * i64::from(ty.bytes()?);
                    iconst(func, args[1]).map(|k| (args[1], k & (bits - 1)))
                }
                _ => None,
            }
        }
        InstData::Icmp { args, .. } => imm(args[1], ty_of(args[0])).map(|k| (args[1], k)),
        InstData::Store { value, .. } if !ty_of(value).is_float() && !ty_of(value).is_vector() => {
            imm(value, ty_of(value)).map(|k| (value, k))
        }
        _ => None,
    }
}

/// The operands of `data` folded into it: the stack address of a load or
/// store, which addresses the slot as `[rbp - n]`, and a [`folded_imm`].
fn folds(func: &Function, data: &InstData) -> Vec<Value> {
    let mut out = Vec::new();
    fold_into(func, data, &mut out);
    out
}

/// [`folds`], appended to `out`.
fn fold_into(func: &Function, data: &InstData, out: &mut Vec<Value>) {
    let wide = |v: Value| func.value_type(func.resolve(v)) == Type::I128;
    let addr = match *data {
        InstData::Load { ty, addr, .. } if ty != Type::I128 => Some(addr),
        InstData::Store { value, addr, .. } if !wide(value) => Some(addr),
        _ => None,
    }
    .filter(|&addr| stack_slot(func, addr).is_some());
    out.extend(addr);
    out.extend(folded_imm(func, data).map(|(v, _)| v));
    out.extend(match *data {
        InstData::Binary { args, .. } if reducible_divisor(func, data).is_some() => {
            Some(func.resolve(args[1]))
        }
        _ => None,
    });
}

/// The stack slot whose address `addr` is, if a `stack_addr` defines it.
fn stack_slot(func: &Function, addr: Value) -> Option<usize> {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(addr)) else {
        return None;
    };
    match *func.inst_data(inst) {
        InstData::StackAddr { slot } => Some(slot.index()),
        _ => None,
    }
}

enum AggregateResult<'a> {
    /// The pieces, the layout, and the frame offset of a fixed destination,
    /// whose address is computed again after the call instead of being kept
    /// in the call's save slot.
    Registers(Vec<sysv::RegPiece>, &'a Layout, Option<usize>),
    X87,
    ScalarMemory,
}

struct FunctionCompiler<'a> {
    goal: crate::CodegenGoal,
    /// One return site owns the epilogue when sharing beats even near jumps.
    shared_return: Option<Inst>,
    code_model: CodeModel,
    features: keel_asm_ir::x86::Features,
    feature_error: Option<String>,
    func: &'a Function,
    symbols: &'a crate::symbols::Symbols<'a>,
    alloc: Allocation,
    current_inst: Option<Inst>,
    allocation_index: Option<usize>,
    selection: crate::select::Selection,
    packed_absolute_values: HashMap<Inst, Value>,
    packed_low_loads: HashMap<Inst, Value>,
    packed_sign_values: HashMap<Inst, packed_sign::Plan>,
    packed_positive_values: HashMap<Inst, Value>,
    /// Offset from `rsp` of each IR stack slot.
    stack_slot_offsets: crate::stack_frame::SlotOffsets,
    /// `stack_addr` and `iconst` results folded into every use, which
    /// need no code.
    folded_values: fold::ValueSet,
    fused_compares: HashSet<Value>,
    /// Bytes at the bottom of the frame for outgoing stack arguments,
    /// rounded to 16; memory from `stack_alloc` starts just above.
    outgoing: usize,
    /// Offset from `rsp` of spill slot 0. Slots past the allocator's are
    /// staging for struct arguments in registers; see [`Self::move_args`].
    spill_base: usize,
    /// Full-copy capacities, independent of a slot's currently live type.
    vector_spills: HashMap<u32, u16>,
    wide_vectors: bool,
    simd_bits: u16,
    frame_align: usize,
    frame_register: Option<u8>,
    copy_temp: Option<u32>,
    /// Offset from `rsp` of an 8-byte slot keeping a call's `sret` address
    /// across the call, for results that come back in registers.
    sret_save: Option<usize>,
    /// Isolated eight-byte object for an outgoing GNU scalar result.
    scalar_result: Option<usize>,
    /// Private MXCSR readback slot for strict-v1 floating rounding.
    mxcsr_save: Option<usize>,
    /// Dedicated aligned old/new raw architectural MXCSR slots.
    mxcsr_access: Option<usize>,
    x87_state: Option<usize>,
    /// Preserve an allocated RBX for fixed-register CX16 and CPUID operations.
    cx16_save: Option<usize>,
    string_io_home: Option<usize>,
    string_io_programs: HashMap<Inst, keel_asm_ir::Program>,
    inline_asm_home: Option<usize>,
    inline_asm_plans: HashMap<Inst, inline_asm::Plan>,
    /// Emission consumes `inline_asm_plans`; this keeps whether there were any.
    has_inline_asm: bool,
    rflags_home: Option<usize>,
    rflags_programs: HashMap<Inst, keel_asm_ir::Program>,
    /// Offset from `rsp` of the slot keeping this function's own `sret`
    /// address, which it returns in rax.
    sret_ptr: Option<usize>,
    /// Offset from `rsp` of the memory behind each `byval` parameter that
    /// arrived in registers, and behind an `sret` result returned in them.
    param_areas: Vec<Option<usize>>,
    /// Offset from `rsp` of a variadic function's register save area:
    /// rdi–r9 (48 bytes), then xmm0–xmm7 (16 bytes each).
    reg_save: Option<usize>,
    /// The named parameters' registers and stack, where `va_start` begins.
    named: sysv::Placement,
    /// Outgoing arguments, spill slots and IR stack slots: what `sub rsp`
    /// allocates below the saved registers.
    frame_size: usize,
    /// A private leaf with no stack frame or callee-save obligations.
    frameless: bool,
    /// The callee-saved registers pushed, in order.
    saved: Vec<u8>,
    /// The block emitted after the current one, which a branch can fall
    /// through to.
    next_block: Option<Block>,
    out: Output,
    uses_trap: bool,
    /// Whether to emit `.loc` directives.
    debug: bool,
    unwind_tables: crate::UnwindTables,
    /// Whether a function gets a section of its own.
    function_sections: bool,
    /// The location of the last `.loc`.
    srcloc: Option<SourceLoc>,
    /// Whether the next `.loc` ends the prologue.
    prologue_end: bool,
    /// The lexical scope of the code being emitted, and where it changed.
    scope: Option<DebugScope>,
    scope_marks: Vec<(String, Option<DebugScope>)>,
    /// The moves of a call's arguments or an edge's block arguments, kept
    /// between parallel copies rather than allocated for each.
    copies: Vec<(Loc, Loc)>,
    /// Where a call's arguments go, kept between calls likewise.
    arg_places: sysv::Placement,
    /// Whether the function's name is written in its labels as it is,
    /// without quotes or escapes.
    bare_name: bool,
}

impl<'a> EdgeCopies<'a> for FunctionCompiler<'a> {
    fn func(&self) -> &'a Function {
        self.func
    }
    fn allocation(&self) -> &Allocation {
        &self.alloc
    }
    fn allocation_index(&self) -> Option<usize> {
        self.allocation_index
    }
    fn current_inst(&self) -> Option<Inst> {
        self.current_inst
    }
    fn copy_buffer(&mut self) -> &mut Vec<(Loc, Loc)> {
        &mut self.copies
    }
    fn emit_parallel_copy(&mut self, moves: &[(Loc, Loc)]) {
        self.parallel_copy(moves);
    }
}

impl<'a> FunctionCompiler<'a> {
    fn cfi(&mut self, directive: Cfi) {
        if self.unwind_tables == crate::UnwindTables::All || self.func.personality.is_some() {
            self.out.directive(Directive::Cfi(directive));
        }
    }

    /// The offset of each stack slot from rbp, where debuggers find the
    /// variables in them: rbp stays put while `rsp` may move.
    fn slot_frame_offsets(&self) -> Vec<i64> {
        if self.frame_register.is_some() {
            return self
                .stack_slot_offsets
                .debug_offsets()
                .map(|o| o as i64)
                .collect();
        }
        let top = (self.frame_size + 8 * self.saved.len()) as i64;
        self.stack_slot_offsets
            .debug_offsets()
            .map(|offset| offset as i64 - top)
            .collect()
    }

    /// Checks the function, then plans it in phases: vector shape, call
    /// area, selection and covers, allocation, spill area, system slots,
    /// inline assembly, IR stack slots and the frame size.
    fn new(
        func: &'a Function,
        symbols: &'a crate::symbols::Symbols<'a>,
        pools: &Pools,
        pipeline: &crate::CodegenPipeline<'_>,
        structured: bool,
        goal: crate::CodegenGoal,
        separate_slots: bool,
    ) -> Result<Self, CodegenError> {
        let module = symbols.module;
        let features = pipeline.selector.x86_features();
        let shape = plan::VectorShape::of(func);
        let saved_pool = pools
            .callee_saved
            .iter()
            .copied()
            .filter(|r| shape.frame_register != Some(*r))
            .collect::<Vec<_>>();
        let simd_pool = pools
            .float_caller_saved
            .iter()
            .copied()
            .filter(|r| !shape.wide_vectors || *r != 12)
            .collect::<Vec<_>>();
        let width_pools = Pools {
            float_caller_saved: &simd_pool,
            callee_saved: &saved_pool,
            ..*pools
        };
        let pools = if shape.frame_register.is_some() {
            &width_pools
        } else {
            pools
        };
        let err = |message: String| CodegenError {
            func: func.name.clone(),
            message,
        };
        plan::preflight(func, features).map_err(err)?;
        let named = sysv::signature_arg_locs(&func.sig, &param_kinds(&func.sig));
        // One walk finds which (mostly rare) instruction kinds occur, so the
        // checks and covers below only scan functions that can match.
        let facts = crate::kinds::FunctionFacts::new(func);
        let present = facts.kinds;
        plan::check_guaranteed_tails(func, &named, present).map_err(err)?;
        let calls = plan::CallArea::plan(func, &named, shape.frame_align);
        let covers = plan::Covers::select(func, pipeline, &facts);
        crate::backend_profile::count_covers(&covers.selection);
        let (alloc, f80_homes) =
            plan::allocate(func, pools, &covers.selection, pipeline).map_err(err)?;
        let spills = plan::SpillArea::plan(func, &alloc, f80_homes, &shape, &calls);
        let system = plan::SystemSlots::plan(func, present, features, spills.end);
        let inline_asm_plans =
            plan::plan_inline_asm(module, func, present, &shape, features).map_err(err)?;
        let asm_bytes = inline_asm_plans
            .values()
            .map(|p| p.bytes)
            .max()
            .unwrap_or(0);
        let inline_asm_home = (asm_bytes != 0).then_some(system.end.next_multiple_of(8));
        let offset = inline_asm_home.map_or(system.end, |home| home + asm_bytes);
        let (stack_slot_offsets, offset) = crate::stack_frame::SlotOffsets::layout_with(
            func,
            &facts.slots,
            offset,
            !module.files.is_empty(),
            separate_slots,
        );
        let saved = plan::saved_registers(
            &alloc,
            system.cx16_save.is_some(),
            &inline_asm_plans,
            shape.frame_register,
        );
        let simple_leaf = plan::simple_leaf(func, symbols, present, features);
        let frame_size =
            plan::frame_size(goal, &shape, simple_leaf, &saved, offset).map_err(err)?;
        let frameless = plan::frameless(
            func,
            goal,
            simple_leaf,
            spills.has_vectors,
            frame_size,
            &named,
            &saved,
        );
        let shared_return = plan::shared_return(func, goal, &alloc, &saved);
        let plan::Covers {
            selection,
            fused_compares,
            packed_absolute_values,
            packed_positive_values,
            packed_sign_values,
            packed_low_loads,
        } = covers;
        let mut compiler = Self {
            goal,
            shared_return,
            code_model: CodeModel::default(),
            func,
            symbols,
            alloc,
            current_inst: None,
            allocation_index: None,
            folded_values: fold::folded_values(func, &selection, |data, out| {
                fold_into(func, data, out)
            }),
            fused_compares,
            packed_low_loads,
            packed_positive_values,
            packed_sign_values,
            packed_absolute_values,
            selection,
            stack_slot_offsets,
            outgoing: calls.outgoing,
            spill_base: spills.spill_base,
            vector_spills: spills.vector_spills,
            wide_vectors: shape.wide_vectors,
            simd_bits: shape.simd_bits,
            frame_align: shape.frame_align,
            frame_register: shape.frame_register,
            copy_temp: spills.copy_temp,
            sret_save: calls.sret_save,
            scalar_result: calls.scalar_result,
            mxcsr_save: system.mxcsr_save,
            mxcsr_access: system.mxcsr_access,
            x87_state: system.x87_state,
            cx16_save: system.cx16_save,
            string_io_home: system.string_io_home,
            string_io_programs: HashMap::default(),
            inline_asm_home,
            has_inline_asm: !inline_asm_plans.is_empty(),
            inline_asm_plans,
            rflags_home: system.rflags_home,
            rflags_programs: HashMap::default(),
            sret_ptr: calls.sret_ptr,
            param_areas: calls.param_areas,
            reg_save: calls.reg_save,
            named,
            frame_size,
            frameless,
            saved,
            next_block: None,
            features,
            feature_error: None,
            out: Output::new(structured),
            uses_trap: false,
            debug: false,
            unwind_tables: crate::UnwindTables::All,
            function_sections: false,
            srcloc: None,
            prologue_end: false,
            scope: None,
            scope_marks: Vec::new(),
            copies: Vec::new(),
            arg_places: sysv::Placement::default(),
            bare_name: bare_label(&func.name),
        };
        compiler.prepare_rflags_regions().map_err(err)?;
        compiler.prepare_string_port_io().map_err(err)?;
        Ok(compiler)
    }

    /// Appends the function's assembly to `out`; gives where its lexical
    /// scopes begin and end.
    fn compile(
        mut self,
        out: &mut Output,
    ) -> Result<Vec<(String, Option<DebugScope>)>, CodegenError> {
        profiling::scope!("instruction-lowering");
        let func = self.func;
        let name = &func.name;
        let symbol = name.clone();
        if func.weak_odr {
            out.directive(Directive::ElfSectionMetadata {
                name: format!(".text.{}", func.name),
                flags: 6 | 0x200,
                nobits: false,
                entry_size: 0,
                group: Some(func.name.clone()),
            });
        } else {
            out.directive(Directive::ElfSection {
                name: crate::emission::elf_text_section(&func.attrs, name, self.function_sections),
                flags: 6 | if func.attrs.retain { 0x200000 } else { 0 },
                nobits: false,
            });
        }
        // On ELF `.weak` alone makes the symbol global; a preceding `.globl`
        // changes its binding twice, which assemblers warn about.
        if !func.local && !func.attrs.weak && !func.weak_odr {
            out.directive(Directive::Global(symbol.clone()));
        }
        if func.attrs.weak || func.weak_odr {
            out.directive(Directive::ElfWeak(symbol.clone()));
        }
        if func.hidden {
            out.directive(Directive::ElfHidden(symbol.clone()));
        }
        let minimum_align = if self.goal == crate::CodegenGoal::Size {
            1
        } else {
            16
        };
        out.align(
            func.attrs
                .align
                .unwrap_or(minimum_align)
                .max(minimum_align)
                .trailing_zeros(),
        );
        out.directive(Directive::ElfType {
            name: symbol.clone(),
            function: true,
        });
        out.label(symbol.clone());
        out.label(format!(".L{name}_begin"));
        self.out.begin_function(out);
        // The prologue takes the function's location, or else that of its
        // first located instruction, with column 0; the first located
        // instruction after it ends the prologue.
        if self.debug {
            let first = func.debug_loc.or_else(|| {
                func.layout_insts()
                    .find_map(|(_, inst)| func.inst_loc(inst))
            });
            self.set_srcloc(Some(SourceLoc {
                file: first.map_or(SourceFile::new(0), |loc| loc.file),
                line: first.map_or(0, |loc| loc.line),
                col: 0,
                scope: None,
            }));
            self.prologue_end = true;
        }
        self.cfi(Cfi::Start);
        if let Some(callee) = (!self.debug)
            .then(|| crate::lowering::identity_tail_callee(func))
            .flatten()
        {
            self.cfi(Cfi::DefCfa {
                register: 7,
                offset: 8,
            });
            self.call_function(Opcode::Jmp, callee);
        } else {
            if self.frameless {
                // The return address remains at rsp; the caller's rbp is untouched.
                self.cfi(Cfi::DefCfa {
                    register: 7,
                    offset: 8,
                });
            } else {
                self.emit(Opcode::Push, [asm_arg(r64(5))]);
                self.cfi(Cfi::DefCfaOffset(16));
                self.cfi(Cfi::Offset {
                    register: 6,
                    offset: -16,
                });
                self.emit(Opcode::Mov, [asm_arg(r64(5)), asm_arg(r64(4))]);
                self.cfi(Cfi::DefCfaRegister(6));
            }
            // Each push is 8 bytes further from the CFA, which is rbp + 16.
            for (k, r) in self.saved.clone().into_iter().enumerate() {
                self.emit(Opcode::Push, [asm_arg(r64(r))]);
                self.cfi(Cfi::Offset {
                    register: dwarf_register(r),
                    offset: -((24 + 8 * k) as i64),
                });
            }
            if self.frame_size > 0 {
                self.emit(Opcode::Sub, [asm_arg(r64(4)), asm_arg(self.frame_size)]);
            }
            if let Some(r) = self.frame_register {
                self.emit(
                    Opcode::And,
                    [asm_arg(r64(4)), asm_arg(-(self.frame_align as i64))],
                );
                self.emit(Opcode::Mov, [asm_arg(r64(r)), asm_arg(r64(4))]);
            }
            self.entry();

            // Only reachable blocks, in the order the allocator numbered them.
            let order = self.alloc.order.clone();
            let aligned = if self.goal == crate::CodegenGoal::Size {
                HashSet::default()
            } else {
                loop_align::headers(func)
            };
            for (i, &block) in order.iter().enumerate() {
                self.next_block = order.get(i + 1).copied();
                if aligned.contains(&block) {
                    // A 32-byte fetch boundary costs at most 31 bytes per header.
                    self.out.align(5);
                }
                self.out.block_label(&func.name, self.bare_name, block);
                for &inst in func.block_insts(block) {
                    self.out.inst_comment(func, inst);
                    if self.debug {
                        self.set_srcloc(func.inst_loc(inst));
                    }
                    self.allocated_inst(inst);
                }
            }
            if self.uses_trap {
                self.out.label(format!(".L{name}_trap"));
                self.emit(Opcode::Ud2, []);
            }
        }
        self.out.label(format!(".L{name}_end"));
        self.cfi(Cfi::End);
        self.out.directive(Directive::Size {
            name: symbol.clone(),
            expression: Expression::difference(".", symbol),
        });
        if let Some(message) = self.feature_error {
            return Err(CodegenError {
                func: name.clone(),
                message,
            });
        }
        self.out.end_function(out);
        Ok(self.scope_marks)
    }

    /// Emits a `.loc` for the code that follows, unless it would repeat
    /// the last one. `None` keeps the last one. A change of lexical scope
    /// is marked with a label.
    fn set_srcloc(&mut self, loc: Option<SourceLoc>) {
        let Some(loc) = loc else { return };
        if loc.scope != self.scope {
            let label = format!(".L{}_ds{}", self.func.name, self.scope_marks.len());
            self.out.label(label.clone());
            self.scope_marks.push((label, loc.scope));
            self.scope = loc.scope;
        }
        if self
            .srcloc
            .is_some_and(|l| (l.file, l.line, l.col) == (loc.file, loc.line, loc.col))
            && !self.prologue_end
        {
            return;
        }
        let SourceLoc {
            file, line, col, ..
        } = loc;
        self.out.directive(Directive::Loc {
            file: (file.index() + 1) as u32,
            line,
            column: col,
            prologue_end: self.prologue_end,
        });
        self.srcloc = Some(loc);
        self.prologue_end = false;
    }

    /// Moves the parameters to their places. Arguments arrive in
    /// registers, some of which are allocatable, so they move as one
    /// parallel copy; before it, nothing may write an argument register,
    /// so what is read from them directly comes first:
    ///
    /// - a variadic function saves every argument register to its save
    ///   area, where `va_arg` finds them (the vector ones only if `al`,
    ///   the count the caller passes, is non-zero);
    /// - an `sret` address in rdi is kept, to return in rax;
    /// - a struct in registers is stored to memory, and its parameter is
    ///   that memory's address.
    ///
    /// Stack parameters are in the caller's frame, above the return
    /// address, and load after the copy: one may be allocated to an
    /// argument register.
    fn entry(&mut self) {
        let func = self.func;
        let Some(entry) = func.entry_block() else {
            return;
        };
        if let Some(area) = self.reg_save {
            for (k, &r) in sysv::INT_ARGS.iter().enumerate() {
                let at = self.frame(area + 8 * k);
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized((at).clone(), 64)), asm_arg(r64(r))],
                );
            }
            let skip = format!(".L{}_saved", func.name);
            self.emit(Opcode::Test, [asm_arg(r8(0)), asm_arg(r8(0))]);
            self.emit(Opcode::Jcc(4), [asm_arg(&skip)]);
            for k in 0..8 {
                let at = self.frame(area + 48 + 16 * k);
                self.emit(
                    Opcode::Movaps,
                    [asm_arg(sized((at).clone(), 128)), asm_arg(xmm((k) as u8))],
                );
            }
            self.out.label(skip);
        }
        if let Some(slot) = self.sret_ptr {
            let at = self.frame(slot);
            self.emit(Opcode::Mov, [asm_arg(sized(at, 64)), asm_arg(r64(7))]);
        }
        let params = func.block_params(entry).to_vec();
        let locs = self.named.locs.clone();
        // What happens after the copy: a stack parameter loads, a struct's
        // parameter gets the address of its bytes.
        enum Later {
            Load(Loc, Type, Memory),
            Address(Loc, Memory),
        }
        let (mut moves, mut later) = (Vec::new(), Vec::new());
        for (i, (param, loc)) in params.into_iter().zip(locs).enumerate() {
            let Some(dst) = self
                .alloc
                .param_loc(entry, crate::regalloc::ValuePart::low(param))
            else {
                continue;
            };
            let abi_param = &func.sig.params[i];
            match loc {
                ArgLoc::Value(Slot::Regs(lo, high)) => {
                    moves.push((dst, lo));
                    if let Some(hi) = self
                        .alloc
                        .param_loc(entry, crate::regalloc::ValuePart::high(param))
                    {
                        moves.push((hi, high.expect("an i128 in two registers")));
                    }
                }
                ArgLoc::Value(Slot::Stack(offset)) => {
                    let at = |o: u32| memory(r64(5), i128::from(16 + offset + o));
                    match self
                        .alloc
                        .param_loc(entry, crate::regalloc::ValuePart::high(param))
                    {
                        Some(hi) => {
                            later.push(Later::Load(dst, Type::I64, at(0)));
                            later.push(Later::Load(hi, Type::I64, at(8)));
                        }
                        None => later.push(Later::Load(dst, abi_param.ty, at(0))),
                    }
                }
                ArgLoc::SretX87 => {
                    let area = self.param_areas[i].expect("planned x87 result object");
                    later.push(Later::Address(dst, self.frame(area)));
                }
                ArgLoc::SretMemory => moves.push((dst, Loc::Reg(RDI))),
                ArgLoc::InRegs(regs)
                    if abi_param.layout().and_then(sysv::register_vector).is_some() =>
                {
                    let ty = abi_param.layout().and_then(sysv::register_vector).unwrap();
                    let area = self.param_areas[i].expect("planned");
                    self.emit(
                        movs(ty),
                        [
                            asm_arg(sized(self.frame(area), ptr(ty))),
                            asm_arg(simd(loc_reg(regs[0].loc), ty)),
                        ],
                    );
                    later.push(Later::Address(dst, self.frame(area)));
                }
                ArgLoc::InRegs(regs) => {
                    let area = self.param_areas[i].expect("planned");
                    for sysv::RegPiece { offset, loc: r } in regs {
                        let at = sized(self.frame(area + offset as usize), 64);
                        match r {
                            Loc::Reg(r) => self.emit(Opcode::Mov, [asm_arg(at), asm_arg(r64(r))]),
                            _ => self.emit(Opcode::Movsd, [asm_arg(at), asm_arg(xmm(loc_reg(r)))]),
                        }
                    }
                    later.push(Later::Address(dst, self.frame(area)));
                }
                ArgLoc::SretRegs(_) => {
                    let area = self.param_areas[i].expect("planned");
                    later.push(Later::Address(dst, self.frame(area)));
                }
                ArgLoc::OnStack(offset) => {
                    later.push(Later::Address(dst, memory(r64(5), i128::from(16 + offset))))
                }
            }
        }
        self.parallel_copy(&moves);
        for step in later {
            match step {
                Later::Load(dst, ty, at) => self.load_into(dst, ty, &at),
                Later::Address(dst, at) => self.address(dst, &at),
            }
        }
    }

    fn selected(&mut self, result: Option<Value>, matched: crate::select::Match) {
        let ty = self.ty(matched.args[0]);
        let a = self.use_reg(matched.args[0], S0);
        // Reuse the same register for both address terms, including a single
        // reload when the input spills. LEA itself neither reads nor sets flags.
        if let crate::select::Machine::LeaMul3 { displacement } = matched.machine {
            let mut at = memory(r64(a), i128::from(displacement));
            at.index = Some(r64(a));
            at.scale = 2;
            let d = self.def_reg(result);
            self.emit(Opcode::Lea, [asm_arg(wide(d, ty)), asm_arg(&at)]);
            self.def_done(result, d);
            return;
        }
        let b = self.use_reg(matched.args[1], S1);
        let d = self.def_reg(result);
        self.emit(
            Opcode::Lea,
            [
                asm_arg(wide(d, ty)),
                asm_arg(indexed(r64(a), r64(b), 1u8 << matched.shift)),
            ],
        );
        self.def_done(result, d);
    }

    #[inline]
    fn allocated_inst(&mut self, inst: Inst) {
        self.current_inst = Some(inst);
        self.allocation_index = self.alloc.edits.instruction_index(inst);
        if let Some(i) = self.allocation_index {
            self.allocation_moves(i, true);
        }
        self.inst(inst);
        if let Some(i) = self.allocation_index {
            self.allocation_moves(i, false);
        }
    }

    // Keep vector copying and cycle resolution out of the default loop's
    // inline path. Existing tiers have no instruction transfers.
    fn allocation_moves(&mut self, index: usize, before: bool) {
        let edit = self.alloc.edits.instruction(index);
        let moves = if before { &edit.before } else { &edit.after };
        if !moves.is_empty() {
            let moves = moves.clone();
            self.parallel_copy(&moves);
        }
    }

    /// Lowers an instruction that selection or a backend cover claimed;
    /// gives whether there was one.
    fn cover(&mut self, inst: Inst, result: Option<Value>) -> bool {
        use crate::cover::Cover;
        let func = self.func;
        if self.selection.covered(inst) {
            return true;
        }
        match self.selection.owner(inst) {
            Some(Cover::PackedAbs) => {
                let value = self.packed_absolute_values[&inst];
                self.vector_absolute(value, result);
            }
            Some(Cover::X86PackedSign) => match self.packed_sign_values[&inst] {
                packed_sign::Plan::Positive(x) => self.vector_positive64(x, result),
                packed_sign::Plan::Select { x, otherwise } => {
                    self.vector_sign_select(x, otherwise, result)
                }
            },
            Some(Cover::X86PackedPositive) => {
                let addr = self.packed_positive_values[&inst];
                self.vector_positive32(addr, result);
            }
            Some(Cover::X86SignedByteLoad) => {
                let addr = self.selection.signed_byte_memory[&inst];
                self.signed_byte_load(addr, result);
            }
            Some(Cover::X86PackedWideLoad) => {
                // Materialize every address leaf before writing the allocated
                // full YMM destination; normal completion handles spills.
                let addr = self.packed_low_loads[&inst];
                let at = self.mem(addr, S0, Type::Vector(keel_ir::VectorType::I32x4));
                let d = self.def_freg(result);
                self.emit(
                    Opcode::V3(keel_asm_ir::x86::V3Opcode::Vpmovsxdq),
                    [asm_arg(simd_width(d, 256)), asm_arg(&at)],
                );
                self.def_done(result, d);
            }
            Some(Cover::X86PackedLowLoad) => {
                let addr = self.packed_low_loads[&inst];
                let at = self.mem(addr, S0, Type::I64);
                let d = self.def_freg(result);
                let opcode = if matches!(
                    func.inst_data(inst),
                    InstData::Convert {
                        op: ConvertOp::SwidenLow,
                        ..
                    }
                ) {
                    Opcode::V2(keel_asm_ir::x86::V2Opcode::Pmovsxdq)
                } else {
                    Opcode::Movq
                };
                self.emit(opcode, [asm_arg(xmm(d)), asm_arg(&at)]);
                self.def_done(result, d);
            }
            Some(Cover::X86LeaMul3) => {
                let matched = self.selection.matches[&inst].clone();
                self.selected(result, matched);
            }
            // The root's own lowering reads these covers' plans. Address
            // covers own the covered `ptradd`, which never reaches here.
            Some(
                Cover::OffsetAddress
                | Cover::BoundedSwitch
                | Cover::IndexedAddress
                | Cover::ZeroTest
                | Cover::X86DivisionRemainder
                | Cover::X86BinaryMemory
                | Cover::X86CompareMemory
                | Cover::CompareBranch
                | Cover::X86MaskTest
                | Cover::X86SelectCompare,
            ) => return false,
            Some(
                cover @ (Cover::Aarch64MaskTest
                | Cover::Aarch64RotateImmediate
                | Cover::Aarch64Ubfx
                | Cover::Aarch64MultiplyShift
                | Cover::Aarch64Abs
                | Cover::Aarch64AbsDiff
                | Cover::Aarch64PackedMinmax
                | Cover::Aarch64ExtendedAdd
                | Cover::Aarch64SelectCompare
                | Cover::Aarch64PackedLowLoad
                | Cover::Aarch64ReplicateLoad
                | Cover::Aarch64VectorImmediate
                | Cover::Aarch64WideningAdd),
            ) => unreachable!("AArch64 cover {} on x86", cover.name()),
            // Generated tree rules and custom selectors.
            None => {
                let Some(matched) = self.selection.matches.get(&inst).cloned() else {
                    return false;
                };
                self.selected(result, matched);
            }
        }
        true
    }

    /// Lowers one instruction: through a cover that claimed it, a wide or
    /// target-specific family, or its instruction family.
    fn inst(&mut self, inst: Inst) {
        let func = self.func;
        let result = func.inst_results(inst).first().copied();
        if self.cover(inst, result) {
            return;
        }
        if self.lower_i128(inst, func.inst_data(inst), result) {
            return;
        }
        if self.avx512_float(func.inst_data(inst), result) {
            return;
        }
        if self.avx512_reduce(func.inst_data(inst), result) {
            return;
        }
        if self.avx512(func.inst_data(inst), result) {
            return;
        }
        if self.avx256(func.inst_data(inst), result) {
            return;
        }
        if self.f80_inst(inst, result) {
            return;
        }
        match *func.inst_data(inst) {
            InstData::VectorIcmp { .. }
            | InstData::VectorSelect { .. }
            | InstData::VectorBinary { .. }
            | InstData::VectorUnary { .. }
            | InstData::VectorShuffle { .. }
            | InstData::VectorExtract { .. }
            | InstData::VectorSplat { .. }
            | InstData::VectorReduce { .. }
            | InstData::VectorInsert { .. }
            | InstData::VectorFcmp { .. } => self.vector_inst(inst, result),
            InstData::Iconst { .. }
            | InstData::F80const { .. }
            | InstData::Fconst { .. }
            | InstData::SymbolAddr { .. }
            | InstData::StackAddr { .. } => self.constant_inst(inst, result),
            InstData::Unary { .. }
            | InstData::Fma { .. }
            | InstData::Overflow { .. }
            | InstData::Binary { .. }
            | InstData::Select { .. }
            | InstData::Convert { .. }
            | InstData::Icmp { .. }
            | InstData::Fcmp { .. } => self.scalar_inst(inst, result),
            InstData::Load { .. }
            | InstData::Store { .. }
            | InstData::AtomicLoad { .. }
            | InstData::AtomicStore { .. }
            | InstData::AtomicRmw { .. }
            | InstData::AtomicCas { .. }
            | InstData::Fence { .. } => self.memory_inst(inst, result),
            InstData::Call { .. }
            | InstData::CallIndirect { .. }
            | InstData::ReturnCall { .. }
            | InstData::ReturnCallIndirect { .. }
            | InstData::Jump { .. }
            | InstData::Brif { .. }
            | InstData::Switch { .. }
            | InstData::Trap
            | InstData::Return { .. } => self.control_inst(inst, result),
            InstData::Intrinsic { id, ref args } => self.intrinsic(inst, id, args, result),
            InstData::InlineAsm { ref args, .. } => self.native_inline_asm(inst, args),
            InstData::X86RflagsRegion { ref args, .. } => self.native_rflags_region(inst, args),
            InstData::VaArg { ty, list, .. } => self.va_arg(inst, ty, list, result),
            InstData::AggregateVaArg {
                ref layout,
                list,
                home,
                x86_vector_abi,
            } => self.aggregate_va_arg(inst, layout, list, home, x86_vector_abi, result),
            InstData::Invoke { .. }
            | InstData::LandingPad { .. }
            | InstData::EhTypeid { .. }
            | InstData::Neon { .. } => unreachable!("rejected in `new`"),
        }
    }

    /// The next variadic argument from the `va_list` at `list`: from the
    /// register save area while registers of its class are left (the
    /// offset is below 48 for general ones, 176 for vector ones), then from
    /// the stack, 8 bytes each. Its address ends up in rax.
    fn va_arg(&mut self, inst: Inst, ty: Type, list: Value, result: Option<Value>) {
        if ty.bytes().is_some_and(|bytes| bytes >= 32)
            || ty == Type::Vector(keel_ir::VectorType::F64x1)
        {
            // __m256 and explicitly proven producer f64x1 are overflow-only.
            // Preflight requires explicit matching local va_start provenance.
            let bytes = ty.bytes().expect("sized overflow vector");
            let p = r64(self.use_reg(list, S2));
            self.emit(
                Opcode::Mov,
                [asm_arg(r64(S0)), asm_arg(sized(memory(p, 8), 64))],
            );
            if bytes >= 32 {
                self.emit(
                    Opcode::Add,
                    [asm_arg(r64(S0)), asm_arg(i64::from(bytes - 1))],
                );
                self.emit(Opcode::And, [asm_arg(r64(S0)), asm_arg(-i64::from(bytes))]);
            }
            self.emit(
                Opcode::Lea,
                [
                    asm_arg(r64(S1)),
                    asm_arg(memory(r64(S0), i128::from(bytes))),
                ],
            );
            self.emit(
                Opcode::Mov,
                [asm_arg(sized(memory(p, 8), 64)), asm_arg(r64(S1))],
            );
            let d = self.def_freg(result);
            self.load_into(Loc::FReg(d), ty, &memory(r64(S0), 0));
            self.def_done(result, d);
            return;
        }
        let p = r64(self.use_reg(list, S2));
        let (field, end, step) = if ty.is_float() || ty.is_vector() {
            (4, 176, 16)
        } else {
            (0, 48, 8)
        };
        let (stack, load) = (
            format!(
                ".L{}_{inst_name}_stack",
                self.func.name,
                inst_name = self.func.inst_name(inst)
            ),
            format!(
                ".L{}_{inst_name}_load",
                self.func.name,
                inst_name = self.func.inst_name(inst)
            ),
        );
        self.emit(
            Opcode::Mov,
            [
                asm_arg(r32(1)),
                asm_arg(sized(memory(p, (field) as i128), 32)),
            ],
        );
        self.emit(Opcode::Cmp, [asm_arg(r32(1)), asm_arg(end)]);
        self.emit(Opcode::Jcc(3), [asm_arg(&stack)]);
        self.emit(
            Opcode::Mov,
            [asm_arg(r64(0)), asm_arg(sized(memory(p, 16i128), 64))],
        );
        self.emit(Opcode::Add, [asm_arg(r64(0)), asm_arg(r64(1))]);
        self.emit(Opcode::Add, [asm_arg(r32(1)), asm_arg(step)]);
        self.emit(
            Opcode::Mov,
            [
                asm_arg(sized(memory(p, (field) as i128), 32)),
                asm_arg(r32(1)),
            ],
        );
        self.emit(Opcode::Jmp, [asm_arg(&load)]);
        self.out.label(stack);
        self.emit(
            Opcode::Mov,
            [asm_arg(r64(0)), asm_arg(sized(memory(p, 8i128), 64))],
        );
        if ty.is_vector() && ty.bytes() == Some(16) {
            self.emit(Opcode::Add, [asm_arg(r64(S0)), asm_arg(15)]);
            self.emit(Opcode::And, [asm_arg(r64(S0)), asm_arg(-16)]);
        }
        self.emit(
            Opcode::Lea,
            [
                asm_arg(r64(S1)),
                asm_arg(memory(
                    r64(S0),
                    if ty.is_vector() {
                        i128::from(ty.bytes().unwrap())
                    } else {
                        8
                    },
                )),
            ],
        );
        self.emit(
            Opcode::Mov,
            [asm_arg(sized(memory(p, 8i128), 64)), asm_arg(r64(1))],
        );
        self.out.label(load);
        if ty.is_vector() {
            let d = self.def_freg(result);
            self.load_into(Loc::FReg(d), ty, &memory(r64(S0), 0));
            self.def_done(result, d);
        } else if ty.is_float() {
            let d = self.def_freg(result);
            self.emit(
                Opcode::Movsd,
                [asm_arg(xmm(d)), asm_arg(sized(memory(r64(0), 0), 64))],
            );
            self.def_done(result, d);
        } else {
            let d = self.def_reg(result);
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(reg(d, ty)),
                    asm_arg(sized(memory(r64(0), 0), ptr(ty))),
                ],
            );
            self.def_done(result, d);
        }
    }

    /// Lowers `data` if it takes or makes an `i128`, and returns whether it
    /// did. An `i128` is a pair of 64-bit halves, each in its own register
    /// or spill slot. x86 instructions take their second operand from
    /// either, so the halves are read where they are ([`Self::half`]); the
    /// result is built in rax (low) and rdx (high) and moved out last, so
    /// every operand is read before the result is written.
    fn lower_i128(&mut self, inst: Inst, data: &InstData, result: Option<Value>) -> bool {
        let wide = |this: &Self, v: Value| this.ty(v) == Type::I128;
        let wide_result = result.is_some_and(|r| self.func.value_type(r) == Type::I128);
        match *data {
            InstData::Iconst {
                ty: Type::I128,
                imm,
            } => {
                self.materialize(S0, imm, Type::I64);
                self.materialize(S2, imm >> 63, Type::I64);
                self.pair_out(result);
            }
            InstData::Binary { op, args } if wide_result => {
                self.binary_i128(inst, op, args);
                self.pair_out(result);
            }
            InstData::Unary { op, arg } if wide_result => {
                self.unary_i128(inst, op, arg);
                self.pair_out(result);
            }
            InstData::Icmp { cond, args } if wide(self, args[0]) => {
                let [a0, a1] = self.halves(args[0]);
                let [b0, b1] = self.halves(args[1]);
                let code = match cond {
                    // Equal if both halves are: their XORs are zero.
                    IntCC::Eq | IntCC::Ne => {
                        self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(a0)]);
                        self.emit(Opcode::Xor, [asm_arg(r64(0)), asm_arg(b0)]);
                        self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a1)]);
                        self.emit(Opcode::Xor, [asm_arg(r64(2)), asm_arg(b1)]);
                        self.emit(Opcode::Or, [asm_arg(r64(0)), asm_arg(r64(2))]);
                        cond_code(cond)
                    }
                    // a - b, with the borrow of the low halves feeding the
                    // high: the flags compare all 128 bits. a > b is b < a,
                    // and a <= b is b >= a: compare swapped.
                    _ => {
                        let swap =
                            matches!(cond, IntCC::Sgt | IntCC::Sle | IntCC::Ugt | IntCC::Ule);
                        let ((x0, x1), (y0, y1)) = if swap {
                            ((b0, b1), (a0, a1))
                        } else {
                            ((a0, a1), (b0, b1))
                        };
                        self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(x0)]);
                        self.emit(Opcode::Cmp, [asm_arg(r64(0)), asm_arg(y0)]);
                        self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(x1)]);
                        self.emit(Opcode::Sbb, [asm_arg(r64(2)), asm_arg(y1)]);
                        match cond {
                            IntCC::Slt | IntCC::Sgt => "l",
                            IntCC::Sle | IntCC::Sge => "ge",
                            IntCC::Ult | IntCC::Ugt => "b",
                            _ => "ae",
                        }
                    }
                };
                let d = self.def_reg(result);
                self.emit(Opcode::Setcc(condition(code)), [asm_arg(r8(d))]);
                self.def_done(result, d);
            }
            InstData::Select { args } if wide(self, args[1]) => {
                if wide(self, args[0]) {
                    let [c0, c1] = self.halves(args[0]);
                    self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(c0)]);
                    self.emit(Opcode::Or, [asm_arg(r64(0)), asm_arg(c1)]);
                } else {
                    self.test_value(args[0]);
                }
                let [t0, t1] = self.halves(args[1]);
                let [f0, f1] = self.halves(args[2]);
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(f0)]);
                self.emit(Opcode::Cmovcc(5), [asm_arg(r64(0)), asm_arg(t0)]);
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(f1)]);
                self.emit(Opcode::Cmovcc(5), [asm_arg(r64(2)), asm_arg(t1)]);
                self.pair_out(result);
            }
            InstData::Convert {
                op: op @ (ConvertOp::Sext | ConvertOp::Uext),
                ty: Type::I128,
                arg,
            } => {
                let from = self.ty(arg);
                let a = self.use_reg(arg, S0);
                let signed = op == ConvertOp::Sext;
                match (from, signed) {
                    (Type::I64, _) => self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(r64(a))]),
                    (Type::I32, true) => {
                        self.emit(Opcode::Movsxd, [asm_arg(r64(0)), asm_arg(r32(a))])
                    }
                    (Type::I32, false) => {
                        self.emit(Opcode::Mov, [asm_arg(r32(0)), asm_arg(r32(a))])
                    }
                    (_, true) => self.emit(Opcode::Movsx, [asm_arg(r64(0)), asm_arg(reg(a, from))]),
                    (_, false) => {
                        self.emit(Opcode::Movzx, [asm_arg(r32(0)), asm_arg(reg(a, from))])
                    }
                }
                if signed {
                    self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(r64(0))]);
                    self.emit(Opcode::Sar, [asm_arg(r64(2)), asm_arg(63i128)]);
                } else {
                    self.emit(Opcode::Xor, [asm_arg(r32(2)), asm_arg(r32(2))]);
                }
                self.pair_out(result);
            }
            InstData::Load {
                ty: Type::I128,
                addr,
                ..
            } => {
                let a = r64(self.use_reg(addr, S1));
                self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(0)), asm_arg(sized(memory(a, 0), 64))],
                );
                self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(2)), asm_arg(sized(memory(a, 8i128), 64))],
                );
                self.pair_out(result);
            }
            InstData::Store { value, addr, .. } if wide(self, value) => {
                let [v0, v1] = self.halves(value);
                let a = r64(self.use_reg(addr, S1));
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(v0)]);
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized(memory(a, 0), 64)), asm_arg(r64(0))],
                );
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(v1)]);
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized(memory(a, 8i128), 64)), asm_arg(r64(0))],
                );
            }
            // The carry or borrow of the low halves feeds the high; the
            // flags then describe the whole 128 bits. Multiplication is
            // rejected in `new`.
            InstData::Overflow { op, args } if wide(self, args[0]) => {
                let [a0, a1] = self.halves(args[0]);
                let [b0, b1] = self.halves(args[1]);
                let (lo, hi, code) = match op {
                    OverflowOp::Uadd => (Opcode::Add, Opcode::Adc, "c"),
                    OverflowOp::Sadd => (Opcode::Add, Opcode::Adc, "o"),
                    OverflowOp::Usub => (Opcode::Sub, Opcode::Sbb, "c"),
                    OverflowOp::Ssub => (Opcode::Sub, Opcode::Sbb, "o"),
                    OverflowOp::Umul | OverflowOp::Smul => unreachable!("rejected"),
                };
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(a0)]);
                self.emit(lo, [asm_arg(r64(0)), asm_arg(b0)]);
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a1)]);
                self.emit(hi, [asm_arg(r64(2)), asm_arg(b1)]);
                self.emit(Opcode::Setcc(condition(code)), [asm_arg(r8(1))]);
                self.pair_out(result);
                let flag = self.func.inst_results(inst)[1];
                if let Some(loc) = self.defined(flag) {
                    self.emit_move(loc, Loc::Reg(S1));
                }
            }
            _ if crate::lowering::wide_atomic(self.func, data) => {
                self.atomic_i128(inst, data, result)
            }
            _ => return false,
        }
        true
    }

    /// The halves of `i128` operand `value`, low first, as operands: a
    /// register or a spill slot.
    fn halves(&self, value: Value) -> [AsmOperand; 2] {
        [self.loc(value), self.hi(value)].map(|loc| self.operand(loc))
    }

    /// Integer location `loc` as a 64-bit operand.
    fn operand(&self, loc: Loc) -> AsmOperand {
        match loc {
            Loc::Reg(r) => asm_arg(r64(r)),
            Loc::Stack(s) => asm_arg(self.slot(s)),
            Loc::FReg(_) => unreachable!("not an integer"),
            Loc::Remat(_) => unreachable!("i128 halves are not rematerialized"),
        }
    }

    /// Moves an `i128` result from rax (low) and rdx (high) to its places.
    fn pair_out(&mut self, result: Option<Value>) {
        let Some(value) = result else { return };
        if let Some(loc) = self.defined(value) {
            self.emit_move(loc, Loc::Reg(S0));
        }
        if let Some(loc) = self.defined_hi(value) {
            self.emit_move(loc, Loc::Reg(S2));
        }
    }

    /// `args[0] op args[1]` on `i128`s into rdx:rax. A shift amount may be
    /// narrower; only its low bits count.
    fn binary_i128(&mut self, inst: Inst, op: BinaryOp, args: [Value; 2]) {
        let [a0, a1] = self.halves(args[0]);
        // Aggregate word packing/unpacking shifts by exactly one eightbyte.
        // Read the needed half before overwriting either result register.
        if matches!(op, BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr)
            && matches!(self.func.value_def(self.func.resolve(args[1])), ValueDef::Result(i, _)
                if matches!(self.func.inst_data(i), InstData::Iconst { imm: 64, .. }))
        {
            if op == BinaryOp::Shl {
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a0)]);
                self.emit(Opcode::Xor, [asm_arg(r32(0)), asm_arg(r32(0))]);
            } else {
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(a1)]);
                if op == BinaryOp::Sshr {
                    self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(r64(0))]);
                    self.emit(Opcode::Sar, [asm_arg(r64(2)), asm_arg(63i128)]);
                } else {
                    self.emit(Opcode::Xor, [asm_arg(r32(2)), asm_arg(r32(2))]);
                }
            }
            return;
        }
        let [b0, b1] = if self.ty(args[1]) == Type::I128 {
            self.halves(args[1])
        } else {
            // A scalar shift/rotate count can be rematerialized, unlike
            // the i128 halves accepted by `operand`.
            let count = match self.loc(args[1]) {
                Loc::Remat(value) => {
                    self.rematerialize(S1, value);
                    asm_arg(r64(S1))
                }
                loc => self.operand(loc),
            };
            [count, asm_arg(0i128)]
        };
        let label = |what: &str| {
            format!(
                ".L{}_{inst_name}_{what}",
                self.func.name,
                inst_name = self.func.inst_name(inst)
            )
        };
        let pair = |this: &mut Self, lo: Opcode, hi: Opcode| {
            this.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(&a0)]);
            this.emit(lo, [asm_arg(r64(0)), asm_arg(&b0)]);
            this.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(&a1)]);
            this.emit(hi, [asm_arg(r64(2)), asm_arg(&b1)]);
        };
        match op {
            BinaryOp::Iadd => pair(self, Opcode::Add, Opcode::Adc),
            BinaryOp::Isub => pair(self, Opcode::Sub, Opcode::Sbb),
            BinaryOp::And => pair(self, Opcode::And, Opcode::And),
            BinaryOp::Or => pair(self, Opcode::Or, Opcode::Or),
            BinaryOp::Xor => pair(self, Opcode::Xor, Opcode::Xor),
            // (a1:a0)(b1:b0) mod 2^128 = a0 b0 + 2^64 (a0 b1 + a1 b0); `mul`
            // gives the full 128-bit a0 b0 in rdx:rax.
            BinaryOp::Imul => {
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(&a0)]);
                self.emit(Opcode::Mul, [asm_arg(&b0)]);
                self.emit(Opcode::Mov, [asm_arg(r64(1)), asm_arg(a0)]);
                self.emit(Opcode::Imul, [asm_arg(r64(1)), asm_arg(b1)]);
                self.emit(Opcode::Add, [asm_arg(r64(2)), asm_arg(r64(1))]);
                self.emit(Opcode::Mov, [asm_arg(r64(1)), asm_arg(a1)]);
                self.emit(Opcode::Imul, [asm_arg(r64(1)), asm_arg(b0)]);
                self.emit(Opcode::Add, [asm_arg(r64(2)), asm_arg(r64(1))]);
            }
            // `shld`/`shrd` shift one half while filling from the other,
            // by the count modulo 64; at 64 or more (bit 6 of the count),
            // the result is the other half shifted, and zeros or the sign.
            BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                let done = label("shifted");
                self.emit(Opcode::Mov, [asm_arg(r64(1)), asm_arg(b0)]);
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(a0)]);
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a1)]);
                match op {
                    BinaryOp::Shl => {
                        self.emit(
                            Opcode::Shld,
                            [asm_arg(r64(2)), asm_arg(r64(0)), asm_arg(r8(1))],
                        );
                        self.emit(Opcode::Shl, [asm_arg(r64(0)), asm_arg(r8(1))]);
                        self.emit(Opcode::Test, [asm_arg(r8(1)), asm_arg(64i128)]);
                        self.emit(Opcode::Jcc(4), [asm_arg(&done)]);
                        self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(r64(0))]);
                        self.emit(Opcode::Xor, [asm_arg(r32(0)), asm_arg(r32(0))]);
                    }
                    _ => {
                        let shr = if op == BinaryOp::Sshr {
                            Opcode::Sar
                        } else {
                            Opcode::Shr
                        };
                        self.emit(
                            Opcode::Shrd,
                            [asm_arg(r64(0)), asm_arg(r64(2)), asm_arg(r8(1))],
                        );
                        self.emit(shr, [asm_arg(r64(2)), asm_arg(r8(1))]);
                        self.emit(Opcode::Test, [asm_arg(r8(1)), asm_arg(64i128)]);
                        self.emit(Opcode::Jcc(4), [asm_arg(&done)]);
                        self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(r64(2))]);
                        if op == BinaryOp::Sshr {
                            self.emit(Opcode::Sar, [asm_arg(r64(2)), asm_arg(63i128)]);
                        } else {
                            self.emit(Opcode::Xor, [asm_arg(r32(2)), asm_arg(r32(2))]);
                        }
                    }
                }
                self.out.label(done);
            }
            // Rotating by 64 swaps the halves; then each half rotates by the
            // rest, filling from the other's original bits.
            BinaryOp::Rotl | BinaryOp::Rotr => {
                let swapped = label("swapped");
                self.emit(Opcode::Mov, [asm_arg(r64(1)), asm_arg(b0)]);
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(a0)]);
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a1)]);
                self.emit(Opcode::Test, [asm_arg(r8(1)), asm_arg(64i128)]);
                self.emit(Opcode::Jcc(4), [asm_arg(&swapped)]);
                self.emit(Opcode::Xchg, [asm_arg(r64(0)), asm_arg(r64(2))]);
                self.out.label(swapped);
                let fill = if op == BinaryOp::Rotl {
                    Opcode::Shld
                } else {
                    Opcode::Shrd
                };
                self.emit(Opcode::Mov, [asm_arg(r64(11)), asm_arg(r64(0))]);
                self.emit(fill, [asm_arg(r64(0)), asm_arg(r64(2)), asm_arg(r8(1))]);
                self.emit(fill, [asm_arg(r64(2)), asm_arg(r64(11)), asm_arg(r8(1))]);
            }
            // libgcc takes the pairs in rdi:rsi and rdx:rcx and returns
            // rdx:rax. Traps first, as for narrower types.
            BinaryOp::Udiv | BinaryOp::Sdiv | BinaryOp::Urem | BinaryOp::Srem => {
                let trap = self.trap_label();
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(&b0)]);
                self.emit(Opcode::Or, [asm_arg(r64(0)), asm_arg(&b1)]);
                self.emit(Opcode::Jcc(4), [asm_arg(&trap)]);
                if op == BinaryOp::Sdiv {
                    // MIN / -1: the divisor's halves are both all ones and
                    // the dividend is 2^127.
                    let ok = label("divok");
                    self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(b0)]);
                    self.emit(Opcode::And, [asm_arg(r64(0)), asm_arg(b1)]);
                    self.emit(Opcode::Cmp, [asm_arg(r64(0)), asm_arg(-1i128)]);
                    self.emit(Opcode::Jcc(5), [asm_arg(&ok)]);
                    self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(a0)]);
                    self.emit(Opcode::Test, [asm_arg(r64(0)), asm_arg(r64(0))]);
                    self.emit(Opcode::Jcc(5), [asm_arg(&ok)]);
                    self.emit(
                        Opcode::Movabs,
                        [asm_arg(r64(0)), asm_arg(0x8000000000000000i128)],
                    );
                    self.emit(Opcode::Cmp, [asm_arg(r64(0)), asm_arg(a1)]);
                    self.emit(Opcode::Jcc(4), [asm_arg(trap)]);
                    self.out.label(ok);
                }
                let moves = [
                    (Loc::Reg(RDI), self.loc(args[0])),
                    (Loc::Reg(6), self.hi(args[0])),
                    (Loc::Reg(RDX), self.loc(args[1])),
                    (Loc::Reg(RCX), self.hi(args[1])),
                ];
                self.parallel_copy(&moves);
                let function = match op {
                    BinaryOp::Udiv => "__udivti3",
                    BinaryOp::Sdiv => "__divti3",
                    BinaryOp::Urem => "__umodti3",
                    _ => "__modti3",
                };
                self.emit(
                    Opcode::Call,
                    [asm_arg(symbol_address(function, Modifier::Plt))],
                );
            }
            _ => unreachable!("{op} on i128 is rejected or not an integer operation"),
        }
    }

    /// `op` on an `i128` into rdx:rax.
    fn unary_i128(&mut self, inst: Inst, op: UnaryOp, arg: Value) {
        let [a0, a1] = self.halves(arg);
        let label = |what: &str| {
            format!(
                ".L{}_{inst_name}_{what}",
                self.func.name,
                inst_name = self.func.inst_name(inst)
            )
        };
        match op {
            // -x = ~x + 1: `neg` the low half sets the carry unless it was
            // zero, which the high half then absorbs.
            UnaryOp::Ineg => {
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(a0)]);
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a1)]);
                self.emit(Opcode::Neg, [asm_arg(r64(0))]);
                self.emit(Opcode::Adc, [asm_arg(r64(2)), asm_arg(0i128)]);
                self.emit(Opcode::Neg, [asm_arg(r64(2))]);
            }
            UnaryOp::Bnot => {
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(a0)]);
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a1)]);
                self.emit(Opcode::Not, [asm_arg(r64(0))]);
                self.emit(Opcode::Not, [asm_arg(r64(2))]);
            }
            // The high half's count, or 64 plus the low half's (see
            // `unary` for the `bsr` arithmetic).
            UnaryOp::Clz => {
                let (low, done) = (label("low"), label("counted"));
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a1)]);
                self.emit(Opcode::Test, [asm_arg(r64(2)), asm_arg(r64(2))]);
                self.emit(Opcode::Jcc(4), [asm_arg(&low)]);
                self.emit(Opcode::Bsr, [asm_arg(r64(0)), asm_arg(r64(2))]);
                self.emit(Opcode::Xor, [asm_arg(r32(0)), asm_arg(63i128)]);
                self.emit(Opcode::Jmp, [asm_arg(&done)]);
                self.out.label(low);
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a0)]);
                self.emit(Opcode::Bsr, [asm_arg(r64(0)), asm_arg(r64(2))]);
                self.emit(Opcode::Mov, [asm_arg(r32(1)), asm_arg(127i128)]);
                self.emit(Opcode::Cmovcc(4), [asm_arg(r32(0)), asm_arg(r32(1))]);
                self.emit(Opcode::Xor, [asm_arg(r32(0)), asm_arg(63i128)]);
                self.emit(Opcode::Add, [asm_arg(r32(0)), asm_arg(64i128)]);
                self.out.label(done);
                self.emit(Opcode::Xor, [asm_arg(r32(2)), asm_arg(r32(2))]);
            }
            UnaryOp::Ctz => {
                let (high, done) = (label("high"), label("counted"));
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a0)]);
                self.emit(Opcode::Test, [asm_arg(r64(2)), asm_arg(r64(2))]);
                self.emit(Opcode::Jcc(4), [asm_arg(&high)]);
                self.emit(Opcode::Bsf, [asm_arg(r64(0)), asm_arg(r64(2))]);
                self.emit(Opcode::Jmp, [asm_arg(&done)]);
                self.out.label(high);
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a1)]);
                self.emit(Opcode::Bsf, [asm_arg(r64(0)), asm_arg(r64(2))]);
                self.emit(Opcode::Mov, [asm_arg(r32(1)), asm_arg(64i128)]);
                self.emit(Opcode::Cmovcc(4), [asm_arg(r32(0)), asm_arg(r32(1))]);
                self.emit(Opcode::Add, [asm_arg(r32(0)), asm_arg(64i128)]);
                self.out.label(done);
                self.emit(Opcode::Xor, [asm_arg(r32(2)), asm_arg(r32(2))]);
            }
            UnaryOp::Popcnt if !self.features.popcnt => {
                // Preserve the second limb and first count outside the GPR scratch set.
                self.emit(Opcode::Movq, [asm_arg(xmm(FS1)), asm_arg(a1)]);
                self.emit(Opcode::Mov, [asm_arg(r64(S0)), asm_arg(a0)]);
                self.software_popcount();
                self.emit(Opcode::Movq, [asm_arg(xmm(FS2)), asm_arg(r64(S0))]);
                self.emit(Opcode::Movq, [asm_arg(r64(S0)), asm_arg(xmm(FS1))]);
                self.software_popcount();
                self.emit(Opcode::Movq, [asm_arg(r64(S1)), asm_arg(xmm(FS2))]);
                self.emit(Opcode::Add, [asm_arg(r64(S0)), asm_arg(r64(S1))]);
                self.emit(Opcode::Xor, [asm_arg(r32(S2)), asm_arg(r32(S2))]);
            }
            UnaryOp::Popcnt => {
                self.emit(Opcode::Popcnt, [asm_arg(r64(0)), asm_arg(a0)]);
                self.emit(Opcode::Popcnt, [asm_arg(r64(2)), asm_arg(a1)]);
                self.emit(Opcode::Add, [asm_arg(r64(0)), asm_arg(r64(2))]);
                self.emit(Opcode::Xor, [asm_arg(r32(2)), asm_arg(r32(2))]);
            }
            UnaryOp::Bswap => {
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(a1)]);
                self.emit(Opcode::Mov, [asm_arg(r64(2)), asm_arg(a0)]);
                self.emit(Opcode::Bswap, [asm_arg(r64(0))]);
                self.emit(Opcode::Bswap, [asm_arg(r64(2))]);
            }
            _ => unreachable!("{op} is not an integer operation"),
        }
    }

    /// A `call` or `jmp` to function `name`: through the PLT if it is in
    /// another module.
    fn call_function(&mut self, opcode: Opcode, name: &str) {
        let plt = !self.symbols.defines(name);
        self.out.branch_to_function(opcode, name, plt);
    }

    /// Moves call arguments into their registers and the stack, for
    /// signature `sig`; any arguments past its parameters are variadic.
    /// `callee`, for an indirect call, moves into r10 in the same parallel
    /// copy, since it may be in an argument register. A tail call (`tail`)
    /// stages stack arguments locally, then copies them to the incoming area
    /// after every source has been captured.
    ///
    /// Stack arguments and struct copies come first: their sources are
    /// allocated locations, and storing them only uses the scratch
    /// registers. A struct passed in registers is loaded into staging
    /// slots, from which the parallel copy fills the registers along with
    /// the scalars: loading it straight into them could overwrite another
    /// argument's source. Values live across the call are in callee-saved
    /// registers or spill slots, so nothing needs saving.
    ///
    /// Narrow arguments marked `sext`/`uext` are extended to 32 bits in
    /// their register, which clang relies on.
    ///
    /// Returns the registers and layout of an `sret` result that comes back
    /// in registers, for [`Self::call_result`] to store.
    fn move_args<'s>(
        &mut self,
        args: &'s keel_ir::CallArgs,
        sig: &'s keel_ir::Signature,
        callee: Option<Value>,
        tail: bool,
    ) -> Option<AggregateResult<'s>> {
        let func = self.func;
        // Live wide values crossing a call have spill homes. Keep upper halves
        // intact whenever an argument still needs its YMM source.
        if self.wide_vectors
            && !args
                .iter()
                .any(|&v| self.ty(v).bytes().is_some_and(|bytes| bytes >= 32))
        {
            self.emit(Opcode::Vzeroupper, []);
        }
        let kind = |i: usize, arg: Value| arg_kind(func, sig, args, i, arg);
        let mut placement = std::mem::take(&mut self.arg_places);
        placement.fill_signature(sig, args.iter().enumerate().map(|(i, &arg)| kind(i, arg)));
        let mut moves = std::mem::take(&mut self.copies);
        let mut extend = Vec::new();
        self.vector_spills
            .retain(|&slot, _| slot < self.alloc.spill_slots || Some(slot) == self.copy_temp);
        let mut staged = self.alloc.spill_slots;
        let mut sret = None;
        // Capture every stack source below our frame before writing any
        // incoming argument byte. Byval pointers may alias or reorder those
        // bytes, or point into local homes that disappear at the jump.
        let stack_base = memory(r64(4), 0);
        let stack_bytes = placement.stack;
        for (i, (&arg, loc)) in args.iter().zip(placement.locs.drain(..)).enumerate() {
            let layout = match kind(i, arg) {
                ArgKind::ByVal(layout)
                | ArgKind::Sret(layout)
                | ArgKind::MemoryByVal(layout)
                | ArgKind::ProducerByVal(layout, _)
                | ArgKind::ProducerSret(layout, _) => Some(layout),
                ArgKind::Value(_) | ArgKind::StackValue(_) => None,
            };
            match loc {
                ArgLoc::Value(Slot::Regs(lo, high)) => {
                    moves.push((lo, self.loc(arg)));
                    if self.ty(arg) == Type::I128 {
                        moves.push((high.expect("an i128 in two registers"), self.hi(arg)));
                    }
                    // Only an extension needs a register's argument again.
                    if let (Some(param), Loc::Reg(r)) =
                        (args.abi(i).or_else(|| sig.params.get(i)), lo)
                        && param.ext != ArgExt::None
                    {
                        extend.push((r, param.ty, param.ext));
                    }
                }
                ArgLoc::Value(Slot::Stack(offset)) => {
                    let at = |o: u32| offset_memory(&stack_base, i128::from(offset + o));
                    if self.ty(arg) == Type::F80 {
                        self.f80_copy(&at(0), &self.f80_home(arg));
                    } else if self.ty(arg) == Type::I128 {
                        self.store_stack_arg(self.loc(arg), &at(0));
                        self.store_stack_arg(self.hi(arg), &at(8));
                    } else if self.ty(arg).is_vector() {
                        let r = self.use_freg(arg, FS0);
                        self.emit(
                            movs(self.ty(arg)),
                            [
                                asm_arg(sized(at(0), ptr(self.ty(arg)))),
                                asm_arg(simd(r, self.ty(arg))),
                            ],
                        );
                    } else {
                        self.store_stack_arg(self.loc(arg), &at(0));
                    }
                }
                ArgLoc::OnStack(offset) => {
                    let p = self.use_reg(arg, S1);
                    let size = layout.expect("a struct").size();
                    self.copy_bytes(&memory(r64(4), i128::from(offset)), p, size);
                }
                ArgLoc::InRegs(regs) if layout.and_then(sysv::register_vector).is_some() => {
                    let ty = layout.and_then(sysv::register_vector).unwrap();
                    let p = self.use_reg(arg, S1);
                    self.emit(
                        movs(ty),
                        [
                            asm_arg(simd(FS0, ty)),
                            asm_arg(sized(memory(r64(p), 0), ptr(ty))),
                        ],
                    );
                    self.emit(
                        movs(ty),
                        [
                            asm_arg(sized(self.slot(staged), ptr(ty))),
                            asm_arg(simd(FS0, ty)),
                        ],
                    );
                    self.vector_spills.insert(staged, ptr(ty));
                    moves.push((regs[0].loc, Loc::Stack(staged)));
                    staged += ty.bytes().unwrap() / 8;
                }
                ArgLoc::InRegs(regs) => {
                    let p = self.use_reg(arg, S1);
                    let size = layout.expect("a struct").size();
                    for sysv::RegPiece { offset, loc: r } in regs {
                        self.load_chunk(S0, p, offset, (size - offset).min(8));
                        self.emit(Opcode::Mov, [asm_arg(self.slot(staged)), asm_arg(r64(0))]);
                        moves.push((r, Loc::Stack(staged)));
                        staged += 1;
                    }
                }
                ArgLoc::SretMemory => moves.push((Loc::Reg(RDI), self.loc(arg))),
                ArgLoc::SretX87 if tail => {}
                ArgLoc::SretX87 => {
                    let p = self.use_reg(arg, S1);
                    let save = self.frame(self.sret_save.expect("planned x87 result destination"));
                    self.emit(Opcode::Mov, [asm_arg(sized(save, 64)), asm_arg(r64(p))]);
                    sret = Some(AggregateResult::X87);
                }
                ArgLoc::SretRegs(_) if tail => {}
                ArgLoc::SretRegs(regs) => {
                    let fixed = self.fixed_result_offset(arg);
                    if fixed.is_none() {
                        let p = self.use_reg(arg, S1);
                        let save = self.frame(self.sret_save.expect("planned"));
                        self.emit(
                            Opcode::Mov,
                            [asm_arg(sized((save).clone(), 64)), asm_arg(r64(p))],
                        );
                    }
                    sret = Some(AggregateResult::Registers(
                        regs,
                        layout.expect("a struct"),
                        fixed,
                    ));
                }
            }
        }
        if let Some(callee) = callee {
            moves.push((Loc::Reg(R10), self.loc(callee)));
        }
        self.parallel_copy(&moves);
        moves.clear();
        self.copies = moves;
        if tail {
            // All sources (including the indirect target and register pieces)
            // have now been captured. RAX is not an argument register; using
            // it here preserves RCX/RDX arguments and R10. The two areas are
            // disjoint, so this is safe for arbitrary incoming-source overlap.
            for offset in (0..stack_bytes).step_by(8) {
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(r64(RAX)),
                        asm_arg(sized(memory(r64(4), i128::from(offset)), 64)),
                    ],
                );
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(sized(memory(r64(5), 16 + i128::from(offset)), 64)),
                        asm_arg(r64(RAX)),
                    ],
                );
            }
        }
        if sysv::scalar_memory_result(sig) {
            if tail {
                // A guaranteed GNU scalar tail forwards the original caller's
                // result object. A local temporary would die with this frame.
                let saved =
                    self.frame(self.sret_ptr.expect("matching incoming GNU result pointer"));
                self.emit(Opcode::Mov, [asm_arg(r64(RDI)), asm_arg(sized(saved, 64))]);
            } else {
                let area = self.frame(self.scalar_result.expect("planned GNU scalar result"));
                self.emit(Opcode::Mov, [asm_arg(sized(area.clone(), 64)), asm_arg(0)]);
                self.emit(Opcode::Lea, [asm_arg(r64(RDI)), asm_arg(area)]);
                sret = Some(AggregateResult::ScalarMemory);
            }
        }
        for (r, ty, ext) in extend {
            self.extend_in_place(r, ty, ext);
        }
        // A variadic callee saves this many vector registers.
        if sig.variadic {
            self.emit(Opcode::Mov, [asm_arg(r32(0)), asm_arg(placement.floats)]);
        }
        self.arg_places = placement;
        sret
    }

    /// The frame offset of the stack slot that `sret` addresses. Inline
    /// assembly keeps the saved pointer, as on AArch64.
    fn fixed_result_offset(&self, sret: Value) -> Option<usize> {
        if self.has_inline_asm {
            return None;
        }
        stack_slot(self.func, sret).map(|slot| self.stack_slot_offsets[slot])
    }

    /// Moves a call's results to their places: [`Self::call_result`] for
    /// at most one, and one parallel copy out of their return registers for
    /// several, which may be allocatable and land in each other's registers.
    fn call_results(
        &mut self,
        inst: Inst,
        sig: &keel_ir::Signature,
        result: Option<Value>,
        sret: Option<AggregateResult<'_>>,
    ) {
        if sig.returns.len() <= 1 {
            return self.call_result(result, sret);
        }
        let func = self.func;
        let locs = sysv::multi_result_locs(sig).expect("preflight checked it");
        let moves: Vec<_> = func
            .inst_results(inst)
            .iter()
            .zip(locs)
            .filter_map(|(&v, src)| self.defined(v).map(|dst| (dst, src)))
            .collect();
        self.parallel_copy(&moves);
    }

    /// Moves a call's result, if it has one, from rax or xmm0 to its place,
    /// and stores a struct result that came back in registers to its
    /// `sret` address.
    fn call_result(&mut self, result: Option<Value>, sret: Option<AggregateResult<'_>>) {
        if matches!(sret, Some(AggregateResult::ScalarMemory)) {
            if let Some(value) = result
                && let Some(loc) = self.defined(value)
            {
                let area = self.frame(self.scalar_result.expect("planned GNU scalar result"));
                self.emit(Opcode::Movsd, [asm_arg(xmm(FS0)), asm_arg(sized(area, 64))]);
                self.emit_move(loc, Loc::FReg(FS0));
            }
        } else if let Some(value) = result.filter(|&v| self.func.value_type(v) == Type::F80) {
            self.f80_pop_result(Some(value));
        } else if let Some(value) = result
            && let Some(loc) = self.defined(value)
        {
            self.emit_move(loc, sysv::return_loc(self.func.value_type(value)));
            if let Some(hi) = self.defined_hi(value) {
                self.emit_move(hi, Loc::Reg(RDX));
            }
        }
        if matches!(sret, Some(AggregateResult::X87)) {
            let save = self.frame(self.sret_save.expect("planned"));
            self.emit(Opcode::Mov, [asm_arg(r64(S3)), asm_arg(sized(save, 64))]);
            self.emit(Opcode::Fstp, [asm_arg(sized(memory(r64(S3), 0), 80))]);
        }
        if let Some(AggregateResult::Registers(regs, layout, fixed)) = sret {
            if let Some(offset) = fixed {
                let at = self.frame(offset);
                self.emit(Opcode::Lea, [asm_arg(r64(S3)), asm_arg(at)]);
            } else {
                let save = self.frame(self.sret_save.expect("planned"));
                self.emit(Opcode::Mov, [asm_arg(r64(S3)), asm_arg(sized(save, 64))]);
            }
            if let Some(ty) = sysv::register_vector(layout) {
                self.emit(
                    movs(ty),
                    [
                        asm_arg(sized(memory(r64(S3), 0), ptr(ty))),
                        asm_arg(simd(loc_reg(regs[0].loc), ty)),
                    ],
                );
            } else {
                for sysv::RegPiece { offset, loc: r } in regs {
                    let n = (layout.size() - offset).min(8);
                    match r {
                        Loc::Reg(r) => self.store_chunk(r, S3, offset, n),
                        _ => {
                            let mov = if n == 4 { Opcode::Movss } else { Opcode::Movsd };
                            let size = if n == 4 { 32 } else { 64 };
                            let x = xmm(loc_reg(r));
                            self.emit(
                                mov,
                                [
                                    asm_arg(sized(memory(r64(11), offset as i128), size)),
                                    asm_arg(x),
                                ],
                            );
                        }
                    }
                }
            }
        }
    }

    /// Loads the `n` bytes (1 to 8) at `[p + offset]` into register `d`,
    /// zero-extended, without reading past them: a struct may end at the
    /// end of a page. Odd sizes are pieced together through rdx.
    fn load_chunk(&mut self, d: u8, p: u8, offset: u64, n: u64) {
        let at = |o: u64| memory(r64(p), i128::from(offset + o));
        match n {
            8 => self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(sized(at(0), 64))]),
            4 => self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(sized(at(0), 32))]),
            2 => self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(sized(at(0), 16))]),
            1 => self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(sized(at(0), 8))]),
            _ => {
                let low = if n > 4 { 4 } else { 2 };
                self.load_chunk(d, p, offset, low);
                for (piece, at_byte) in pieces(n - low).into_iter().map(|(o, k)| (k, low + o)) {
                    self.load_chunk(S2, p, offset + at_byte, piece);
                    self.emit(Opcode::Shl, [asm_arg(r64(2)), asm_arg(8 * at_byte)]);
                    self.emit(Opcode::Or, [asm_arg(r64(d)), asm_arg(r64(2))]);
                }
            }
        }
    }

    /// Stores the low `n` bytes (1 to 8) of register `r` at
    /// `[p + offset]`, without writing past them. `r` is shifted down as
    /// pieces are written.
    fn store_chunk(&mut self, r: u8, p: u8, offset: u64, n: u64) {
        let mut done = 0;
        for (o, k) in pieces(n) {
            if o > done {
                self.emit(Opcode::Shr, [asm_arg(r64(r)), asm_arg(8 * (o - done))]);
                done = o;
            }
            let (size, src) = match k {
                8 => (64, r64(r)),
                4 => (32, r32(r)),
                2 => (16, r16(r)),
                _ => (8, r8(r)),
            };
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(sized(memory(r64(p), (offset + o) as i128), size)),
                    asm_arg(src),
                ],
            );
        }
    }

    /// Copies `size` bytes from `[src]` to `[to]`, 8 at a time through rax
    /// and then the remainder exactly.
    fn copy_bytes(&mut self, to: &Memory, src: u8, size: u64) {
        let mut offset = 0;
        while offset < size {
            let n = (size - offset).min(8);
            for (o, k) in pieces(n) {
                let (ptr_kw, rax) = match k {
                    8 => (64, r64(0)),
                    4 => (32, r32(0)),
                    2 => (16, r16(0)),
                    _ => (8, r8(0)),
                };
                let at = offset + o;
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(rax),
                        asm_arg(sized(memory(r64(src), (at) as i128), ptr_kw)),
                    ],
                );
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(sized(offset_memory(to, (at) as i128), ptr_kw)),
                        asm_arg(rax),
                    ],
                );
            }
            offset += n;
        }
    }

    /// Sets `dst` to the address of memory operand `at` (`[rbp - 16]`).
    fn address(&mut self, dst: Loc, at: &Memory) {
        match dst {
            Loc::Reg(r) => self.emit(Opcode::Lea, [asm_arg(r64(r)), asm_arg(at)]),
            _ => {
                self.emit(Opcode::Lea, [asm_arg(r64(0)), asm_arg(at)]);
                self.emit_move(dst, Loc::Reg(S0));
            }
        }
    }

    /// Extends the narrow value in register `r` to 32 bits, if `ext` asks.
    fn extend_in_place(&mut self, r: u8, ty: Type, ext: ArgExt) {
        let op = match ext {
            ArgExt::None => return,
            ArgExt::Sext => Opcode::Movsx,
            ArgExt::Uext => Opcode::Movzx,
        };
        if matches!(ty, Type::I8 | Type::I16) {
            self.emit(op, [asm_arg(r32(r)), asm_arg(reg(r, ty))]);
        }
    }

    /// Stores an argument from `src` at memory `at`, a stack argument's
    /// slot. Each has 8 bytes, so all 64 bits of an integer can go.
    fn store_stack_arg(&mut self, src: Loc, at: &Memory) {
        match src {
            Loc::Reg(r) => self.emit(
                Opcode::Mov,
                [asm_arg(sized((at).clone(), 64)), asm_arg(r64(r))],
            ),
            Loc::FReg(r) => self.emit(
                Opcode::Movsd,
                [asm_arg(sized((at).clone(), 64)), asm_arg(xmm(r))],
            ),
            Loc::Stack(s) => {
                self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(0)), asm_arg(sized(self.slot(s), 64))],
                );
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized((at).clone(), 64)), asm_arg(r64(0))],
                );
            }
            Loc::Remat(value) => {
                self.rematerialize(S0, value);
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized(at.clone(), 64)), asm_arg(r64(S0))],
                );
            }
        }
    }

    /// Loads a value of type `ty` from memory `src` into `dst`.
    fn load_into(&mut self, dst: Loc, ty: Type, src: &Memory) {
        if ty == Type::F80 {
            let Loc::Stack(slot) = dst else {
                unreachable!("F80 has a stack home")
            };
            self.f80_copy(&self.slot(slot), src);
            return;
        }
        match dst {
            Loc::Reg(r) => self.emit(
                Opcode::Mov,
                [asm_arg(r64(r)), asm_arg(sized((src).clone(), 64))],
            ),
            Loc::FReg(r) => self.emit(
                movs(ty),
                [asm_arg(simd(r, ty)), asm_arg(sized((src).clone(), ptr(ty)))],
            ),
            Loc::Stack(_) => {
                let tmp = if ty.is_float() || ty.is_vector() {
                    Loc::FReg(FS0)
                } else {
                    Loc::Reg(S0)
                };
                self.load_into(tmp, ty, src);
                self.emit_move(dst, tmp);
            }
            Loc::Remat(_) => unreachable!("rematerialized locations cannot be load destinations"),
        }
    }

    /// Restores the callee-saved registers, rsp and rbp.
    fn epilogue(&mut self) {
        if self.frameless {
            return;
        }
        if self.saved.is_empty() {
            if self.goal == crate::CodegenGoal::Size {
                self.emit(Opcode::Leave, []);
                return;
            }
            self.emit(Opcode::Mov, [asm_arg(r64(4)), asm_arg(r64(5))]);
        } else {
            self.emit(
                Opcode::Lea,
                [
                    asm_arg(r64(4)),
                    asm_arg(memory(r64(5), -((8 * self.saved.len()) as i128))),
                ],
            );
            for r in self.saved.clone().into_iter().rev() {
                self.emit(Opcode::Pop, [asm_arg(r64(r))]);
            }
        }
        self.emit(Opcode::Pop, [asm_arg(r64(5))]);
    }

    /// Computes `args[0] op args[1]` into `d`.
    fn binary(&mut self, inst: Inst, op: BinaryOp, args: [Value; 2], d: u8) {
        let ty = self.ty(args[0]);
        let a = self.use_reg(args[0], S0);
        if let Some(&addr) = self.selection.binary_memory.get(&inst) {
            // A coalesced lhs already holds the accumulator. mem only reloads
            // S1/S2, and the ALU reads the address before writing its result,
            // so d == a is safe even when lhs also supplies an address leaf.
            // Otherwise preserve the conservative S0/late-copy path: writing
            // a distinct d early could destroy an allocated address leaf.
            let accumulator = if d == a { a } else { S0 };
            if a != accumulator {
                self.emit(Opcode::Mov, [asm_arg(r64(accumulator)), asm_arg(r64(a))]);
            }
            let at = self.mem(addr, S1, ty);
            let mnemonic = match op {
                BinaryOp::Iadd => Opcode::Add,
                BinaryOp::Isub => Opcode::Sub,
                BinaryOp::And => Opcode::And,
                BinaryOp::Or => Opcode::Or,
                BinaryOp::Xor => Opcode::Xor,
                BinaryOp::Imul => Opcode::Imul,
                _ => unreachable!("selected integer memory RHS"),
            };
            self.emit(mnemonic, [asm_arg(reg(accumulator, ty)), asm_arg(&at)]);
            // IR narrow arithmetic is modular. Normalize the partial-register
            // result explicitly before any subsequent extension or spill.
            if matches!(ty, Type::I8 | Type::I16) {
                self.emit(
                    Opcode::Movzx,
                    [asm_arg(r32(accumulator)), asm_arg(reg(accumulator, ty))],
                );
            }
            if d != accumulator {
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(accumulator))]);
            }
            return;
        }
        let imm = folded_imm(self.func, self.func.inst_data(inst)).map(|(_, k)| k);
        let simple = match op {
            BinaryOp::Iadd | BinaryOp::Ptradd => Some(Opcode::Add),
            BinaryOp::Isub => Some(Opcode::Sub),
            BinaryOp::And => Some(Opcode::And),
            BinaryOp::Or => Some(Opcode::Or),
            BinaryOp::Xor => Some(Opcode::Xor),
            BinaryOp::Imul if imm.is_none() => Some(Opcode::Imul),
            _ => None,
        };
        if let Some(mnemonic) = simple {
            let b = match imm {
                Some(k) => Operand::Imm(k),
                None => Operand::Reg(self.use_reg(args[1], S1)),
            };
            self.two_address(mnemonic, ty, d, a, b);
            return;
        }
        match op {
            // The three-operand form writes a fresh destination.
            BinaryOp::Imul => {
                let k = imm.expect("register form handled above");
                self.emit(
                    Opcode::Imul,
                    [asm_arg(wide(d, ty)), asm_arg(wide(a, ty)), asm_arg(k)],
                );
            }
            BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr | BinaryOp::Rotl | BinaryOp::Rotr => {
                if self.features.bmi2 {
                    if imm.is_none()
                        && matches!(op, BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr)
                    {
                        let mut b = self.use_reg(args[1], S1);
                        // Only narrow types need an explicit modulo mask. Copy
                        // the count before masking, preserving any live input.
                        if let Some(bits) = narrow_bits(ty) {
                            if b != S1 {
                                self.emit(Opcode::Mov, [asm_arg(r32(S1)), asm_arg(r32(b))]);
                            }
                            self.emit(Opcode::And, [asm_arg(r32(S1)), asm_arg(bits - 1)]);
                            b = S1;
                        }
                        let a = match op {
                            BinaryOp::Ushr => self.extend(S0, a, ty, false),
                            BinaryOp::Sshr => self.extend(S0, a, ty, true),
                            _ => a,
                        };
                        let mnemonic = match op {
                            BinaryOp::Shl => Opcode::Shlx,
                            BinaryOp::Ushr => Opcode::Shrx,
                            _ => Opcode::Sarx,
                        };
                        self.emit(
                            mnemonic,
                            [
                                asm_arg(wide(d, ty)),
                                asm_arg(wide(a, ty)),
                                asm_arg(wide(b, ty)),
                            ],
                        );
                        return;
                    }
                    if let Some(k) = imm
                        && narrow_bits(ty).is_none()
                        && matches!(op, BinaryOp::Rotl | BinaryOp::Rotr)
                    {
                        let bits = if wide_ty(ty) == Type::I64 { 64 } else { 32 };
                        let count = if op == BinaryOp::Rotl { -k } else { k } & (bits - 1);
                        self.emit(
                            Opcode::Rorx,
                            [asm_arg(wide(d, ty)), asm_arg(wide(a, ty)), asm_arg(count)],
                        );
                        return;
                    }
                }
                let mnemonic = match op {
                    BinaryOp::Shl => Opcode::Shl,
                    BinaryOp::Ushr => Opcode::Shr,
                    BinaryOp::Sshr => Opcode::Sar,
                    BinaryOp::Rotl => Opcode::Rol,
                    _ => Opcode::Ror,
                };
                // The count goes in cl. Shifts take it modulo 32 or 64; a
                // narrower type needs its own mask. Rotates of narrow
                // registers already rotate modulo their width.
                let count = match imm {
                    Some(k) => asm_arg(k),
                    None => {
                        let b = self.use_reg(args[1], S1);
                        if b != RCX {
                            self.emit(Opcode::Mov, [asm_arg(r32(1)), asm_arg(r32(b))]);
                        }
                        let rotate = matches!(op, BinaryOp::Rotl | BinaryOp::Rotr);
                        if let (Some(bits), false) = (narrow_bits(ty), rotate) {
                            self.emit(Opcode::And, [asm_arg(r32(1)), asm_arg(bits - 1)]);
                        }
                        asm_arg(r8(1))
                    }
                };
                // Right shifts read the upper bits, so a narrow value is
                // extended first; rotates work at the type's own width.
                let (a, width) = match op {
                    BinaryOp::Ushr => (self.extend(S0, a, ty, false), wide_ty(ty)),
                    BinaryOp::Sshr => (self.extend(S0, a, ty, true), wide_ty(ty)),
                    BinaryOp::Shl => (a, wide_ty(ty)),
                    _ => (a, ty),
                };
                if a != d {
                    self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(a))]);
                }
                self.emit(mnemonic, [asm_arg(reg(d, width)), asm_arg(count)]);
            }
            // The double-width product of the extended operands: rdx:rax
            // from a one-operand `mul`/`imul` for 32 and 64 bits; for
            // narrower types, the product fits 32 bits, shifted down.
            BinaryOp::Umulhi | BinaryOp::Smulhi => {
                let signed = op == BinaryOp::Smulhi;
                let b = self.use_reg(args[1], S1);
                match narrow_bits(ty) {
                    None => {
                        if a != S0 {
                            self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(r64(a))]);
                        }
                        let mnemonic = if signed { Opcode::Imul } else { Opcode::Mul };
                        self.emit(mnemonic, [asm_arg(reg(b, ty))]);
                        self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(2))]);
                    }
                    Some(bits) => {
                        let x = self.extend(S0, a, ty, signed);
                        let y = self.extend(S1, b, ty, signed);
                        if x != S0 {
                            self.emit(Opcode::Mov, [asm_arg(r32(0)), asm_arg(r32(x))]);
                        }
                        self.emit(Opcode::Imul, [asm_arg(r32(0)), asm_arg(r32(y))]);
                        let shr = if signed { Opcode::Sar } else { Opcode::Shr };
                        self.emit(shr, [asm_arg(r32(0)), asm_arg(bits)]);
                        self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(0))]);
                    }
                }
            }
            BinaryOp::Udiv | BinaryOp::Urem | BinaryOp::Sdiv | BinaryOp::Srem => {
                self.divide(inst, op, args, a, d);
            }
            _ => unreachable!("{op} is not an integer operation"),
        }
    }

    /// Division: the dividend in rax (edx:eax or rdx:rax extended from
    /// it), the divisor in a register; the quotient comes back in rax, the
    /// remainder in rdx. Narrow types are extended to 32 bits first.
    fn divide(&mut self, inst: Inst, op: BinaryOp, args: [Value; 2], a: u8, d: u8) {
        let ty = self.ty(args[0]);
        let signed = matches!(op, BinaryOp::Sdiv | BinaryOp::Srem);
        let width = wide_ty(ty);
        if let Some(divisor) = reducible_divisor(self.func, self.func.inst_data(inst)) {
            self.constant_division(op, ty, a, divisor, d);
            return;
        }
        let b = self.use_reg(args[1], S1);
        let b = self.extend(S1, b, ty, signed);
        let x = self.extend(S0, a, ty, signed);
        if x != S0 {
            self.emit(
                Opcode::Mov,
                [asm_arg(reg(S0, width)), asm_arg(reg(x, width))],
            );
        }
        let rb = reg(b, width);
        let trap = self.trap_label();
        // Hardware division faults on a zero divisor; the IR traps.
        self.emit(Opcode::Test, [asm_arg(rb), asm_arg(rb)]);
        self.emit(Opcode::Jcc(4), [asm_arg(&trap)]);
        let name = &self.func.name;
        let done = format!(
            ".L{name}_{inst_name}_div",
            inst_name = self.func.inst_name(inst)
        );
        if signed {
            // MIN / -1 overflows, which faults too. The IR traps on the
            // quotient and gives 0 for the remainder.
            self.emit(Opcode::Cmp, [asm_arg(rb), asm_arg(-1i128)]);
            let go = format!(
                ".L{name}_{inst_name}_divide",
                inst_name = self.func.inst_name(inst)
            );
            self.emit(Opcode::Jcc(5), [asm_arg(&go)]);
            if op == BinaryOp::Sdiv {
                let min = min_value(ty);
                if width == Type::I64 {
                    self.emit(Opcode::Movabs, [asm_arg(r64(2)), asm_arg(min)]);
                    self.emit(Opcode::Cmp, [asm_arg(r64(0)), asm_arg(r64(2))]);
                } else {
                    self.emit(Opcode::Cmp, [asm_arg(r32(0)), asm_arg(min as u32 as i32)]);
                }
                self.emit(Opcode::Jcc(4), [asm_arg(trap)]);
            } else {
                self.emit(Opcode::Xor, [asm_arg(r32(2)), asm_arg(r32(2))]);
                self.emit(Opcode::Jmp, [asm_arg(&done)]);
            }
            self.out.label(go);
            self.emit(
                if width == Type::I64 {
                    Opcode::Cqo
                } else {
                    Opcode::Cdq
                },
                [],
            );
            self.emit(Opcode::Idiv, [asm_arg(rb)]);
        } else {
            self.emit(Opcode::Xor, [asm_arg(r32(2)), asm_arg(r32(2))]);
            self.emit(Opcode::Div, [asm_arg(rb)]);
        }
        self.out.label(done);
        let src = match op {
            BinaryOp::Udiv | BinaryOp::Sdiv => S0,
            _ => S2,
        };
        self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(src))]);
    }

    /// `d = a op b` with a two-address instruction `op d, b`, for an
    /// integer operation of type `ty`, on 32 or 64 bits.
    fn two_address(&mut self, mnemonic: Opcode, ty: Type, d: u8, a: u8, b: Operand) {
        let width = wide_ty(ty);
        // These integer operations commute at the emitted width, including
        // their defined flags (IMUL defines only CF/OF). If b already occupies
        // d, operate on it directly instead of saving a through S0. Keep this
        // allowlist explicit: subtraction and other two-address ops cannot swap.
        if matches!(b, Operand::Reg(r) if r == d)
            && a != d
            && matches!(
                mnemonic,
                Opcode::Add | Opcode::And | Opcode::Or | Opcode::Xor | Opcode::Imul
            )
        {
            self.emit(mnemonic, [asm_arg(reg(d, width)), asm_arg(reg(a, width))]);
            return;
        }
        // A sum into a third register is one LEA. Nothing reads an IR add's
        // flags, and a 32-bit LEA truncates like a 32-bit ADD, so a 32-bit
        // constant may be taken as signed.
        if mnemonic == Opcode::Add && a != d {
            let at = match b {
                Operand::Reg(r) if r != d => Some(indexed(r64(a), r64(r), 1)),
                Operand::Imm(k) if width != Type::I64 => Some(memory(r64(a), (k as i32).into())),
                Operand::Imm(k) => i32::try_from(k).ok().map(|k| memory(r64(a), k.into())),
                Operand::Reg(_) => None,
            };
            if let Some(at) = at {
                self.emit(Opcode::Lea, [asm_arg(reg(d, width)), asm_arg(&at)]);
                return;
            }
        }
        // `mov d, a` would overwrite b.
        let dst = match b {
            Operand::Reg(r) if r == d && r != a => S0,
            _ => d,
        };
        if dst != a {
            self.emit(Opcode::Mov, [asm_arg(r64(dst)), asm_arg(r64(a))]);
        }
        let b = match b {
            Operand::Reg(r) => asm_arg(reg(r, width)),
            Operand::Imm(k) => asm_arg(k),
        };
        self.emit(mnemonic, [asm_arg(reg(dst, width)), asm_arg(b)]);
        if dst != d {
            self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(dst))]);
        }
    }

    fn unary(&mut self, op: UnaryOp, arg: Value, d: u8) {
        let ty = self.ty(arg);
        let a = self.use_reg(arg, S0);
        let width = wide_ty(ty);
        match op {
            UnaryOp::Ineg | UnaryOp::Bnot => {
                if a != d {
                    self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(a))]);
                }
                let mnemonic = if op == UnaryOp::Ineg {
                    Opcode::Neg
                } else {
                    Opcode::Not
                };
                self.emit(mnemonic, [asm_arg(reg(d, width))]);
            }
            // `bsr` gives the index of the highest set bit, i, and leaves
            // ZF set for zero. The count is (bits - 1) - i, which is
            // (bits - 1) ^ i; for zero, (2 * bits - 1) ^ (bits - 1) = bits.
            // Narrow values count as 32 bits, less the bits above them.
            UnaryOp::Clz => {
                let x = self.extend(S0, a, ty, false);
                let bits = if width == Type::I64 { 64 } else { 32 };
                let (rx, rax) = (reg(x, width), reg(S0, width));
                self.emit(Opcode::Bsr, [asm_arg(rax), asm_arg(rx)]);
                self.emit(Opcode::Mov, [asm_arg(r32(1)), asm_arg(2 * bits - 1)]);
                self.emit(Opcode::Cmovcc(4), [asm_arg(rax), asm_arg(reg(S1, width))]);
                self.emit(Opcode::Xor, [asm_arg(r32(0)), asm_arg(bits - 1)]);
                if let Some(narrow) = narrow_bits(ty) {
                    self.emit(Opcode::Sub, [asm_arg(r32(0)), asm_arg(32 - narrow)]);
                }
                self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(0))]);
            }
            // `bsf` gives the index of the lowest set bit. A narrow value
            // gets a bit set just above it, which is the answer for zero.
            UnaryOp::Ctz if self.features.bmi1 => {
                if let Some(bits) = narrow_bits(ty) {
                    // Sentinel forces zero to return the IR type's width;
                    // bits above it cannot influence the first set bit.
                    self.emit(Opcode::Mov, [asm_arg(r32(S0)), asm_arg(r32(a))]);
                    self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(1u32 << bits)]);
                    self.emit(Opcode::Tzcnt, [asm_arg(r32(d)), asm_arg(r32(S0))]);
                } else {
                    self.emit(
                        Opcode::Tzcnt,
                        [asm_arg(reg(d, width)), asm_arg(reg(a, width))],
                    );
                }
            }
            UnaryOp::Ctz => match narrow_bits(ty) {
                Some(bits) => {
                    self.emit(Opcode::Mov, [asm_arg(r32(0)), asm_arg(r32(a))]);
                    self.emit(Opcode::Or, [asm_arg(r32(0)), asm_arg(1u32 << bits)]);
                    self.emit(Opcode::Bsf, [asm_arg(r32(0)), asm_arg(r32(0))]);
                    self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(0))]);
                }
                None => {
                    let bits = if width == Type::I64 { 64 } else { 32 };
                    self.emit(
                        Opcode::Bsf,
                        [asm_arg(reg(S0, width)), asm_arg(reg(a, width))],
                    );
                    self.emit(Opcode::Mov, [asm_arg(r32(1)), asm_arg(bits)]);
                    self.emit(
                        Opcode::Cmovcc(4),
                        [asm_arg(reg(S0, width)), asm_arg(reg(S1, width))],
                    );
                    self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(S0))]);
                }
            },
            UnaryOp::Popcnt if !self.features.popcnt => {
                let x = self.extend(S0, a, ty, false);
                // A 32-bit write also removes the unspecified upper half of i32.
                self.emit(
                    Opcode::Mov,
                    [asm_arg(reg(S0, width)), asm_arg(reg(x, width))],
                );
                self.software_popcount();
                self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S0))]);
            }
            UnaryOp::Popcnt => {
                let x = self.extend(S0, a, ty, false);
                self.emit(
                    Opcode::Popcnt,
                    [asm_arg(reg(d, width)), asm_arg(reg(x, width))],
                );
            }
            UnaryOp::Bswap => {
                if a != d {
                    self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(a))]);
                }
                match ty {
                    Type::I8 => {}
                    Type::I16 => self.emit(Opcode::Rol, [asm_arg(r16(d)), asm_arg(8i128)]),
                    _ => self.emit(Opcode::Bswap, [asm_arg(reg(d, ty))]),
                }
            }
            _ => unreachable!("{op} is not an integer operation"),
        }
    }

    /// SWAR population count. Only reserved GPRs are clobbered; result is in rax.
    fn software_popcount(&mut self) {
        self.emit(Opcode::Mov, [asm_arg(r64(S1)), asm_arg(r64(S0))]);
        self.emit(Opcode::Shr, [asm_arg(r64(S1)), asm_arg(1)]);
        self.emit(
            Opcode::Movabs,
            [asm_arg(r64(S2)), asm_arg(0x5555555555555555i128)],
        );
        self.emit(Opcode::And, [asm_arg(r64(S1)), asm_arg(r64(S2))]);
        self.emit(Opcode::Sub, [asm_arg(r64(S0)), asm_arg(r64(S1))]);
        self.emit(Opcode::Mov, [asm_arg(r64(S1)), asm_arg(r64(S0))]);
        self.emit(Opcode::Shr, [asm_arg(r64(S1)), asm_arg(2)]);
        self.emit(
            Opcode::Movabs,
            [asm_arg(r64(S2)), asm_arg(0x3333333333333333i128)],
        );
        self.emit(Opcode::And, [asm_arg(r64(S0)), asm_arg(r64(S2))]);
        self.emit(Opcode::And, [asm_arg(r64(S1)), asm_arg(r64(S2))]);
        self.emit(Opcode::Add, [asm_arg(r64(S0)), asm_arg(r64(S1))]);
        self.emit(Opcode::Mov, [asm_arg(r64(S1)), asm_arg(r64(S0))]);
        self.emit(Opcode::Shr, [asm_arg(r64(S1)), asm_arg(4)]);
        self.emit(Opcode::Add, [asm_arg(r64(S0)), asm_arg(r64(S1))]);
        self.emit(
            Opcode::Movabs,
            [asm_arg(r64(S2)), asm_arg(0x0f0f0f0f0f0f0f0fi128)],
        );
        self.emit(Opcode::And, [asm_arg(r64(S0)), asm_arg(r64(S2))]);
        for shift in [8, 16, 32] {
            self.emit(Opcode::Mov, [asm_arg(r64(S1)), asm_arg(r64(S0))]);
            self.emit(Opcode::Shr, [asm_arg(r64(S1)), asm_arg(shift)]);
            self.emit(Opcode::Add, [asm_arg(r64(S0)), asm_arg(r64(S1))]);
        }
        self.emit(Opcode::And, [asm_arg(r32(S0)), asm_arg(127)]);
    }

    /// Round IEEE binary32/binary64 by manipulating the significand. No integer
    /// conversion, floating rounding-mode dependence, or hidden library call.
    /// Preserve the sign of zero and pass NaNs through SSE arithmetic to quiet
    /// signaling inputs with the same invalid exception behavior as ROUNDSS/SD.
    fn software_round(&mut self, inst: Inst, op: UnaryOp, ty: Type, a: u8, d: u8) {
        let (mantissa, bias, sign, mov) = if ty == Type::F32 {
            (23, 127, 31, Opcode::Movd)
        } else {
            (52, 1023, 63, Opcode::Movq)
        };
        let bits_reg = |n| if ty == Type::F32 { r32(n) } else { r64(n) };
        let label = |suffix: &str| {
            format!(
                ".L{}_{inst_name}_round_{suffix}",
                self.func.name,
                inst_name = self.func.inst_name(inst)
            )
        };
        let (small, truncate, increment, sign_result, done, nan) = (
            label("small"),
            label("truncate"),
            label("increment"),
            label("sign"),
            label("done"),
            label("nan"),
        );
        // FS1 retains the original bits, FS2 the unsigned magnitude.
        self.emit(Opcode::Movaps, [asm_arg(xmm(FS1)), asm_arg(xmm(a))]);
        self.emit(mov, [asm_arg(bits_reg(S0)), asm_arg(xmm(FS1))]);
        self.emit(Opcode::Btr, [asm_arg(r64(S0)), asm_arg(sign)]);
        self.emit(mov, [asm_arg(xmm(FS2)), asm_arg(bits_reg(S0))]);
        self.emit(Opcode::Mov, [asm_arg(r64(S1)), asm_arg(r64(S0))]);
        self.emit(Opcode::Shr, [asm_arg(r64(S1)), asm_arg(mantissa)]);
        // Large finite values and infinities are integral. NaNs use SSE below.
        self.emit(Opcode::Cmp, [asm_arg(r64(S1)), asm_arg(2 * bias + 1)]);
        self.emit(Opcode::Jcc(4), [asm_arg(&nan)]);
        self.emit(Opcode::Cmp, [asm_arg(r64(S1)), asm_arg(bias + mantissa)]);
        self.emit(Opcode::Jcc(3), [asm_arg(&sign_result)]); // unsigned >=
        self.emit(Opcode::Cmp, [asm_arg(r64(S1)), asm_arg(bias)]);
        self.emit(Opcode::Jcc(2), [asm_arg(&small)]);
        // unit = 1 << (mantissa - unbiased_exponent).
        self.emit(Opcode::Neg, [asm_arg(r64(S1))]);
        self.emit(Opcode::Add, [asm_arg(r64(S1)), asm_arg(bias + mantissa)]);
        self.emit(Opcode::Mov, [asm_arg(r32(S2)), asm_arg(1)]);
        self.emit(Opcode::Shl, [asm_arg(r64(S2)), asm_arg(r8(S1))]);
        self.emit(Opcode::Sub, [asm_arg(r64(S2)), asm_arg(1)]);
        self.emit(Opcode::And, [asm_arg(r64(S0)), asm_arg(r64(S2))]); // remainder
        self.emit(Opcode::Add, [asm_arg(r64(S2)), asm_arg(1)]); // unit
        self.emit(Opcode::Test, [asm_arg(r64(S0)), asm_arg(r64(S0))]);
        self.emit(Opcode::Jcc(4), [asm_arg(&truncate)]);
        match op {
            UnaryOp::Fnearest => {
                self.emit(Opcode::Mov, [asm_arg(r64(S1)), asm_arg(r64(S2))]);
                self.emit(Opcode::Shr, [asm_arg(r64(S1)), asm_arg(1)]);
                self.emit(Opcode::Cmp, [asm_arg(r64(S0)), asm_arg(r64(S1))]);
                self.emit(Opcode::Jcc(7), [asm_arg(&increment)]);
                self.emit(Opcode::Jcc(2), [asm_arg(&truncate)]);
                self.emit(mov, [asm_arg(bits_reg(S0)), asm_arg(xmm(FS2))]);
                self.emit(Opcode::Test, [asm_arg(r64(S0)), asm_arg(r64(S2))]);
                self.emit(Opcode::Jcc(5), [asm_arg(&increment)]); // ties to even
            }
            UnaryOp::Ffloor | UnaryOp::Fceil => {
                self.emit(mov, [asm_arg(bits_reg(S0)), asm_arg(xmm(FS1))]);
                self.emit(Opcode::Bt, [asm_arg(r64(S0)), asm_arg(sign)]);
                self.emit(
                    Opcode::Jcc(if op == UnaryOp::Ffloor { 2 } else { 3 }),
                    [asm_arg(&increment)],
                );
            }
            _ => {}
        }
        self.emit(Opcode::Jmp, [asm_arg(&truncate)]);
        self.out.label(increment);
        self.emit(mov, [asm_arg(bits_reg(S0)), asm_arg(xmm(FS2))]);
        self.emit(Opcode::Add, [asm_arg(r64(S0)), asm_arg(r64(S2))]);
        self.emit(mov, [asm_arg(xmm(FS2)), asm_arg(bits_reg(S0))]);
        self.out.label(truncate);
        self.emit(mov, [asm_arg(bits_reg(S0)), asm_arg(xmm(FS2))]);
        self.emit(Opcode::Neg, [asm_arg(r64(S2))]); // -unit clears fractional bits
        self.emit(Opcode::And, [asm_arg(r64(S0)), asm_arg(r64(S2))]);
        self.emit(Opcode::Jmp, [asm_arg(&sign_result)]);
        self.out.label(small);
        // ROUND treats subnormal input as signed zero when MXCSR.DAZ is set.
        // Read MXCSR without modifying it, using a private fixed-frame slot.
        // This leaves stack/unwind state unchanged at the operation.
        let normal_small = label("normal_small");
        self.emit(Opcode::Test, [asm_arg(r64(S1)), asm_arg(r64(S1))]);
        self.emit(Opcode::Jcc(5), [asm_arg(&normal_small)]);
        let mxcsr = sized(self.frame(self.mxcsr_save.expect("planned MXCSR slot")), 32);
        self.emit(Opcode::Stmxcsr, [asm_arg(mxcsr.clone())]);
        self.emit(Opcode::Mov, [asm_arg(r32(S1)), asm_arg(mxcsr)]);
        self.emit(Opcode::Test, [asm_arg(r32(S1)), asm_arg(64)]);
        self.emit(Opcode::Jcc(4), [asm_arg(&normal_small)]);
        self.emit(Opcode::Xor, [asm_arg(r32(S0)), asm_arg(r32(S0))]);
        self.emit(Opcode::Jmp, [asm_arg(&sign_result)]);
        self.out.label(normal_small);
        // For |x| < 1, the result is signed zero or signed one.
        self.emit(Opcode::Mov, [asm_arg(r64(S1)), asm_arg(r64(S0))]);
        self.emit(Opcode::Xor, [asm_arg(r32(S0)), asm_arg(r32(S0))]);
        let small_done = label("small_done");
        if op == UnaryOp::Fnearest {
            self.emit(
                Opcode::Movabs,
                [asm_arg(r64(S2)), asm_arg(((bias - 1) as i128) << mantissa)],
            );
            self.emit(Opcode::Cmp, [asm_arg(r64(S1)), asm_arg(r64(S2))]);
            self.emit(Opcode::Jcc(6), [asm_arg(&small_done)]); // <= half, including tie
        } else if op == UnaryOp::Ftrunc {
            self.emit(Opcode::Jmp, [asm_arg(&small_done)]);
        } else {
            self.emit(Opcode::Test, [asm_arg(r64(S1)), asm_arg(r64(S1))]);
            self.emit(Opcode::Jcc(4), [asm_arg(&small_done)]);
            self.emit(mov, [asm_arg(bits_reg(S1)), asm_arg(xmm(FS1))]);
            self.emit(Opcode::Bt, [asm_arg(r64(S1)), asm_arg(sign)]);
            self.emit(
                Opcode::Jcc(if op == UnaryOp::Ffloor { 3 } else { 2 }),
                [asm_arg(&small_done)],
            );
        }
        self.emit(
            Opcode::Movabs,
            [asm_arg(r64(S0)), asm_arg((bias as i128) << mantissa)],
        );
        self.out.label(small_done);
        self.out.label(sign_result);
        self.emit(mov, [asm_arg(bits_reg(S1)), asm_arg(xmm(FS1))]);
        self.emit(Opcode::Shr, [asm_arg(r64(S1)), asm_arg(sign)]);
        self.emit(Opcode::Shl, [asm_arg(r64(S1)), asm_arg(sign)]);
        self.emit(Opcode::Or, [asm_arg(r64(S0)), asm_arg(r64(S1))]);
        self.emit(mov, [asm_arg(xmm(d)), asm_arg(bits_reg(S0))]);
        self.emit(Opcode::Jmp, [asm_arg(&done)]);
        self.out.label(nan);
        self.emit(Opcode::Movaps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
        self.emit(
            scalar(Opcode::Addsd, ty),
            [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))],
        );
        self.emit(Opcode::Movaps, [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
        self.out.label(done);
    }

    /// Float negation, absolute value and copysign flip or clear the sign
    /// bit; the rest are SSE instructions (`roundsd` is SSE4.1).
    fn float_unary(&mut self, inst: Inst, op: UnaryOp, arg: Value, d: u8) {
        let ty = self.ty(arg);
        let a = self.use_freg(arg, FS0);
        let (gpr, sign) = if ty == Type::F32 {
            (r32(0), 31)
        } else {
            (r64(0), 63)
        };
        let mov = if ty == Type::F32 {
            Opcode::Movd
        } else {
            Opcode::Movq
        };
        match op {
            UnaryOp::Fneg | UnaryOp::Fabs => {
                let bit_op = if op == UnaryOp::Fneg {
                    Opcode::Btc
                } else {
                    Opcode::Btr
                };
                self.emit(mov, [asm_arg(gpr), asm_arg(xmm(a))]);
                self.emit(bit_op, [asm_arg(gpr), asm_arg(sign)]);
                self.emit(mov, [asm_arg(xmm(d)), asm_arg(gpr)]);
            }
            UnaryOp::Fsqrt if self.features.avx => self.emit(
                if ty == Type::F32 {
                    Opcode::Vsqrtss
                } else {
                    Opcode::Vsqrtsd
                },
                [asm_arg(xmm(d)), asm_arg(xmm(a)), asm_arg(xmm(a))],
            ),
            UnaryOp::Fsqrt => self.emit(
                scalar(Opcode::Sqrtsd, ty),
                [asm_arg(xmm(d)), asm_arg(xmm(a))],
            ),
            _ if !self.features.sse41 => self.software_round(inst, op, ty, a, d),
            _ => {
                // Bit 3 suppresses the inexact exception; bits 1:0 are the
                // rounding mode.
                let mode = match op {
                    UnaryOp::Fnearest => 8,
                    UnaryOp::Ffloor => 9,
                    UnaryOp::Fceil => 10,
                    UnaryOp::Ftrunc => 11,
                    _ => unreachable!("{op} is not a float operation"),
                };
                self.emit(
                    scalar(Opcode::Roundsd, ty),
                    [asm_arg(xmm(d)), asm_arg(xmm(a)), asm_arg(mode)],
                );
            }
        }
    }

    fn float_binary(&mut self, inst: Inst, op: BinaryOp, args: [Value; 2], d: u8) {
        let ty = self.ty(args[0]);
        let a = self.use_freg(args[0], FS0);
        let b = self.use_freg(args[1], FS1);

        if self.features.avx {
            let opcode = match (op, ty == Type::F32) {
                (BinaryOp::Fadd, true) => Some(Opcode::Vaddss),
                (BinaryOp::Fadd, false) => Some(Opcode::Vaddsd),
                (BinaryOp::Fsub, true) => Some(Opcode::Vsubss),
                (BinaryOp::Fsub, false) => Some(Opcode::Vsubsd),
                (BinaryOp::Fmul, true) => Some(Opcode::Vmulss),
                (BinaryOp::Fmul, false) => Some(Opcode::Vmulsd),
                (BinaryOp::Fdiv, true) => Some(Opcode::Vdivss),
                (BinaryOp::Fdiv, false) => Some(Opcode::Vdivsd),
                _ => None,
            };
            if let Some(opcode) = opcode {
                self.emit(opcode, [asm_arg(xmm(d)), asm_arg(xmm(a)), asm_arg(xmm(b))]);
                return;
            }
        }

        // Two-address, computed in xmm13 unless d is free to overwrite.
        let dst = if b == d && a != d { FS0 } else { d };
        let (xd, xb) = (xmm(dst), xmm(b));
        if dst != a {
            self.emit(Opcode::Movaps, [asm_arg(xd), asm_arg(xmm(a))]);
        }
        match op {
            BinaryOp::Fadd | BinaryOp::Fsub | BinaryOp::Fmul | BinaryOp::Fdiv => {
                let mnemonic = match op {
                    BinaryOp::Fadd => Opcode::Add,
                    BinaryOp::Fsub => Opcode::Sub,
                    BinaryOp::Fmul => Opcode::Mul,
                    _ => Opcode::Div,
                };
                self.emit(scalar(mnemonic, ty), [asm_arg(xd), asm_arg(xb)]);
            }
            // `minsd`/`maxsd` give the second operand for a NaN or two
            // zeros. IEEE minimum wants NaN, and -0 below +0: for equal
            // operands, OR the bits (min) or AND them (max), which only
            // differs from either operand for zeros of opposite signs.
            BinaryOp::Fmin | BinaryOp::Fmax => {
                let min = op == BinaryOp::Fmin;
                let name = &self.func.name;
                let (nan, ne, done) = (
                    format!(
                        ".L{name}_{inst_name}_nan",
                        inst_name = self.func.inst_name(inst)
                    ),
                    format!(
                        ".L{name}_{inst_name}_ne",
                        inst_name = self.func.inst_name(inst)
                    ),
                    format!(
                        ".L{name}_{inst_name}_done",
                        inst_name = self.func.inst_name(inst)
                    ),
                );

                self.emit(scalar(Opcode::Ucomisd, ty), [asm_arg(xd), asm_arg(xb)]);
                self.emit(Opcode::Jcc(10), [asm_arg(&nan)]);
                self.emit(Opcode::Jcc(5), [asm_arg(&ne)]);
                let bits = if min { Opcode::Or } else { Opcode::And };
                self.emit(packed(bits, ty), [asm_arg(xd), asm_arg(xb)]);
                self.emit(Opcode::Jmp, [asm_arg(&done)]);
                self.out.label(ne);
                let pick = scalar(if min { Opcode::Minsd } else { Opcode::Maxsd }, ty);
                self.emit(pick, [asm_arg(xd), asm_arg(xb)]);
                self.emit(Opcode::Jmp, [asm_arg(&done)]);
                // Adding gives a quiet NaN.
                self.out.label(nan);
                self.emit(scalar(Opcode::Addsd, ty), [asm_arg(xd), asm_arg(xb)]);
                self.out.label(done);
            }
            // The sign bit of b with the rest of a, spliced in rax.
            BinaryOp::Fcopysign => {
                let (mov, ra, rc, sign) = if ty == Type::F32 {
                    (Opcode::Movd, r32(0), r32(1), 31)
                } else {
                    (Opcode::Movq, r64(0), r64(1), 63)
                };
                self.emit(mov, [asm_arg(ra), asm_arg(xd)]);
                self.emit(mov, [asm_arg(rc), asm_arg(xb)]);
                self.emit(Opcode::Btr, [asm_arg(ra), asm_arg(sign)]);
                self.emit(Opcode::Shr, [asm_arg(rc), asm_arg(sign)]);
                self.emit(Opcode::Shl, [asm_arg(rc), asm_arg(sign)]);
                self.emit(Opcode::Or, [asm_arg(ra), asm_arg(rc)]);
                self.emit(mov, [asm_arg(xd), asm_arg(ra)]);
            }
            _ => unreachable!("{op} is not a float operation"),
        }
        if dst != d {
            self.emit(Opcode::Movaps, [asm_arg(xmm(d)), asm_arg(xd)]);
        }
    }

    fn float_convert(
        &mut self,
        inst: Inst,
        op: ConvertOp,
        to: Type,
        arg: Value,
        result: Option<Value>,
    ) {
        let from = self.ty(arg);
        let name = self.func.name.clone();
        let label = |what: &str| {
            format!(
                ".L{name}_{inst_name}_{what}",
                inst_name = self.func.inst_name(inst)
            )
        };
        match op {
            ConvertOp::Fpext | ConvertOp::Fptrunc => {
                let d = self.def_freg(result);
                let a = self.use_freg(arg, FS0);
                let mnemonic = if op == ConvertOp::Fpext {
                    Opcode::Cvtss2sd
                } else {
                    Opcode::Cvtsd2ss
                };
                self.emit(mnemonic, [asm_arg(xmm(d)), asm_arg(xmm(a))]);
                self.def_done(result, d);
            }
            // `cvtsi2sd` reads a signed 32- or 64-bit integer. Narrow and
            // unsigned 32-bit values are extended to 64 bits first, which
            // makes them non-negative where needed.
            ConvertOp::Sitofp | ConvertOp::Uitofp => {
                let signed = op == ConvertOp::Sitofp;
                let d = self.def_freg(result);
                let a = self.use_reg(arg, S0);

                let xd = xmm(d);
                // `cvtsi2sd` only writes the low lane; clearing the
                // register first breaks the dependency on its old value.
                self.emit(Opcode::Xorps, [asm_arg(xd), asm_arg(xd)]);
                match (from, signed) {
                    (Type::I64, true) => {
                        self.emit(scalar(Opcode::Cvtsi2sd, to), [asm_arg(xd), asm_arg(r64(a))])
                    }
                    (_, true) => {
                        let x = self.extend(S0, a, from, true);
                        self.emit(scalar(Opcode::Cvtsi2sd, to), [asm_arg(xd), asm_arg(r32(x))]);
                    }
                    (Type::I64, false) => {
                        // Below 2^63 the signed conversion is right. Above,
                        // halve (keeping the low bit, so rounding is still
                        // right), convert and double.
                        let (big, done) = (label("big"), label("done"));
                        if a != S0 {
                            self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(r64(a))]);
                        }
                        self.emit(Opcode::Test, [asm_arg(r64(0)), asm_arg(r64(0))]);
                        self.emit(Opcode::Jcc(8), [asm_arg(&big)]);
                        self.emit(scalar(Opcode::Cvtsi2sd, to), [asm_arg(xd), asm_arg(r64(0))]);
                        self.emit(Opcode::Jmp, [asm_arg(&done)]);
                        self.out.label(big);
                        self.emit(Opcode::Mov, [asm_arg(r64(1)), asm_arg(r64(0))]);
                        self.emit(Opcode::Shr, [asm_arg(r64(1)), asm_arg(1i128)]);
                        self.emit(Opcode::And, [asm_arg(r32(0)), asm_arg(1i128)]);
                        self.emit(Opcode::Or, [asm_arg(r64(1)), asm_arg(r64(0))]);
                        self.emit(scalar(Opcode::Cvtsi2sd, to), [asm_arg(xd), asm_arg(r64(1))]);
                        self.emit(scalar(Opcode::Addsd, to), [asm_arg(xd), asm_arg(xd)]);
                        self.out.label(done);
                    }
                    (_, false) => {
                        let x = self.extend(S0, a, from, false);
                        // A 32-bit move zero-extends to 64.
                        self.emit(Opcode::Mov, [asm_arg(r32(0)), asm_arg(r32(x))]);
                        self.emit(scalar(Opcode::Cvtsi2sd, to), [asm_arg(xd), asm_arg(r64(0))]);
                    }
                }
                self.def_done(result, d);
            }
            ConvertOp::Fptosi | ConvertOp::Fptoui => {
                let d = self.def_reg(result);
                let a = self.use_freg(arg, FS0);
                self.float_to_int(op == ConvertOp::Fptosi, from, to, a, &label);
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(0))]);
                self.def_done(result, d);
            }
            // The same bits in the other register class.
            ConvertOp::Bitcast => {
                let mov = if from == Type::F32 || to == Type::F32 {
                    Opcode::Movd
                } else {
                    Opcode::Movq
                };
                if to.is_float() {
                    let d = self.def_freg(result);
                    let a = self.use_reg(arg, S0);
                    self.emit(mov, [asm_arg(xmm(d)), asm_arg(reg(a, from))]);
                    self.def_done(result, d);
                } else {
                    let d = self.def_reg(result);
                    let a = self.use_freg(arg, FS0);
                    self.emit(mov, [asm_arg(reg(d, to)), asm_arg(xmm(a))]);
                    self.def_done(result, d);
                }
            }
            _ => unreachable!("{op} is not a float conversion"),
        }
    }

    /// Float `xmm<a>` to an integer of type `to` in rax, rounding toward
    /// zero and saturating, with NaN giving 0. `cvttsd2si` itself gives
    /// the "integer indefinite", the minimum, for NaN and out-of-range
    /// values, which the compares then correct.
    fn float_to_int(
        &mut self,
        signed: bool,
        from: Type,
        to: Type,
        a: u8,
        label: &dyn Fn(&str) -> String,
    ) {
        let xa = xmm(a);
        let to64 = to == Type::I64;
        // The bits of 2^n as a float of type `from`, in xmm14.
        let power = |this: &mut Self, n: i32| {
            let bits = if from == Type::F32 {
                u64::from(2f32.powi(n).to_bits())
            } else {
                2f64.powi(n).to_bits()
            };
            this.emit(Opcode::Movabs, [asm_arg(r64(11)), asm_arg(bits)]);
            this.emit(Opcode::Movq, [asm_arg(xmm(FS1)), asm_arg(r64(11))]);
        };
        if signed {
            let (rax, rcx) = if to64 {
                (r64(0), r64(1))
            } else {
                (r32(0), r32(1))
            };
            let bits = if to64 { 63 } else { 31 };
            self.emit(scalar(Opcode::Cvttsd2si, from), [asm_arg(rax), asm_arg(xa)]);
            power(self, bits);
            self.emit(
                scalar(Opcode::Ucomisd, from),
                [asm_arg(xa), asm_arg(xmm(FS1))],
            );
            // At or above 2^bits (and not NaN): the maximum.
            self.emit(
                Opcode::Movabs,
                [asm_arg(r64(1)), asm_arg((1u64 << bits) - 1)],
            );
            self.emit(Opcode::Cmovcc(3), [asm_arg(rax), asm_arg(rcx)]);
            self.emit(Opcode::Mov, [asm_arg(r32(1)), asm_arg(0i128)]);
            self.emit(Opcode::Cmovcc(10), [asm_arg(rax), asm_arg(rcx)]);
            return;
        }
        if !to64 {
            // Every u32 is an in-range i64. Negative (or NaN, or huge,
            // which convert to the minimum) gives 0; then clamp above.
            self.emit(
                scalar(Opcode::Cvttsd2si, from),
                [asm_arg(r64(0)), asm_arg(xa)],
            );
            self.emit(Opcode::Mov, [asm_arg(r32(1)), asm_arg(0i128)]);
            self.emit(Opcode::Test, [asm_arg(r64(0)), asm_arg(r64(0))]);
            self.emit(Opcode::Cmovcc(8), [asm_arg(r64(0)), asm_arg(r64(1))]);
            self.emit(Opcode::Mov, [asm_arg(r32(1)), asm_arg(0xffffffffi128)]);
            self.emit(Opcode::Cmp, [asm_arg(r64(0)), asm_arg(r64(1))]);
            self.emit(Opcode::Cmovcc(7), [asm_arg(r64(0)), asm_arg(r64(1))]);
            power(self, 32);
            self.emit(
                scalar(Opcode::Ucomisd, from),
                [asm_arg(xa), asm_arg(xmm(FS1))],
            );
            self.emit(Opcode::Cmovcc(3), [asm_arg(r64(0)), asm_arg(r64(1))]);
            return;
        }
        // Below 2^63 (or NaN) the signed conversion, clamped at 0. At or
        // above, convert x - 2^63 and set the top bit back, unless that
        // overflows too, which is the maximum.
        let (big, max, done) = (label("big"), label("max"), label("done"));
        power(self, 63);
        self.emit(
            scalar(Opcode::Ucomisd, from),
            [asm_arg(xa), asm_arg(xmm(FS1))],
        );
        self.emit(Opcode::Jcc(3), [asm_arg(&big)]);
        self.emit(
            scalar(Opcode::Cvttsd2si, from),
            [asm_arg(r64(0)), asm_arg(xa)],
        );
        self.emit(Opcode::Mov, [asm_arg(r32(1)), asm_arg(0i128)]);
        self.emit(Opcode::Test, [asm_arg(r64(0)), asm_arg(r64(0))]);
        self.emit(Opcode::Cmovcc(8), [asm_arg(r64(0)), asm_arg(r64(1))]);
        self.emit(Opcode::Jmp, [asm_arg(&done)]);
        self.out.label(big);
        self.emit(Opcode::Movaps, [asm_arg(xmm(FS2)), asm_arg(xa)]);
        self.emit(
            scalar(Opcode::Subsd, from),
            [asm_arg(xmm(FS2)), asm_arg(xmm(FS1))],
        );
        self.emit(
            scalar(Opcode::Cvttsd2si, from),
            [asm_arg(r64(0)), asm_arg(xmm(FS2))],
        );
        self.emit(Opcode::Test, [asm_arg(r64(0)), asm_arg(r64(0))]);
        self.emit(Opcode::Jcc(8), [asm_arg(&max)]);
        self.emit(Opcode::Btc, [asm_arg(r64(0)), asm_arg(63i128)]);
        self.emit(Opcode::Jmp, [asm_arg(&done)]);
        self.out.label(max);
        self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(-1i128)]);
        self.out.label(done);
    }

    /// The value and overflow flag of `op`. The value is computed in rax
    /// and the flag in cl, then both move to their places.
    fn overflow(&mut self, inst: Inst, op: OverflowOp, args: [Value; 2]) {
        let ty = self.ty(args[0]);
        let signed = matches!(op, OverflowOp::Sadd | OverflowOp::Ssub | OverflowOp::Smul);
        let a = self.use_reg(args[0], S0);
        let a = self.extend(S0, a, ty, signed);
        let b = self.use_reg(args[1], S1);
        let b = self.extend(S1, b, ty, signed);
        let width = wide_ty(ty);
        if a != S0 {
            self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(r64(a))]);
        }
        let rb = reg(b, width);
        match op {
            OverflowOp::Umul => self.emit(Opcode::Mul, [asm_arg(rb)]),
            OverflowOp::Smul => self.emit(Opcode::Imul, [asm_arg(reg(S0, width)), asm_arg(rb)]),
            OverflowOp::Uadd | OverflowOp::Sadd => {
                self.emit(Opcode::Add, [asm_arg(reg(S0, width)), asm_arg(rb)])
            }
            OverflowOp::Usub | OverflowOp::Ssub => {
                self.emit(Opcode::Sub, [asm_arg(reg(S0, width)), asm_arg(rb)])
            }
        }
        match narrow_bits(ty) {
            // The 32-bit result of the extended operands overflows the
            // narrow type if it differs from its own extension.
            Some(_) => {
                let x = self.extend(S1, S0, ty, signed);
                self.emit(Opcode::Cmp, [asm_arg(r32(0)), asm_arg(r32(x))]);
                self.emit(Opcode::Setcc(5), [asm_arg(r8(1))]);
            }
            None => {
                let cc = match op {
                    OverflowOp::Uadd | OverflowOp::Usub | OverflowOp::Umul => "c",
                    _ => "o",
                };
                self.emit(Opcode::Setcc(condition(cc)), [asm_arg(r8(1))]);
            }
        }
        let results = self.func.inst_results(inst);
        let (value, flag) = (results[0], results[1]);
        if let Some(loc) = self.defined(value) {
            self.emit_move(loc, Loc::Reg(S0));
        }
        if let Some(loc) = self.defined(flag) {
            self.emit_move(loc, Loc::Reg(S1));
        }
    }

    /// `lock xadd` adds and gives the old value; `xchg` exchanges. The
    /// other operations have no instruction that gives the old value, so
    /// they loop: compute the new value from the old one in rax, and
    /// `lock cmpxchg` it in if memory still holds the old one (else rax
    /// gets the current value and it tries again).
    fn atomic_rmw(&mut self, inst: Inst, op: RmwOp, args: [Value; 2], result: Option<Value>) {
        let ty = self.ty(args[1]);
        let a = self.use_reg(args[0], S2);
        let v = self.use_reg(args[1], S3);
        let at = sized(memory(r64(a), 0), ptr(ty));
        match op {
            RmwOp::Add | RmwOp::Sub | RmwOp::Xchg => {
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(r64(v))]);
                if op == RmwOp::Sub {
                    self.emit(Opcode::Neg, [asm_arg(reg(S0, ty))]);
                }
                let operands = [asm_arg(&at), asm_arg(reg(S0, ty))];
                if op == RmwOp::Xchg {
                    self.emit(Opcode::Xchg, operands);
                } else {
                    self.emit_locked(Opcode::Xadd, operands);
                }
            }
            _ => {
                let retry = format!(
                    ".L{}_{inst_name}_retry",
                    self.func.name,
                    inst_name = self.func.inst_name(inst)
                );
                match ty {
                    Type::I8 | Type::I16 => {
                        self.emit(Opcode::Movzx, [asm_arg(r32(0)), asm_arg(&at)])
                    }
                    _ => self.emit(Opcode::Mov, [asm_arg(reg(S0, ty)), asm_arg(&at)]),
                }
                self.out.label(retry.clone());
                self.emit(Opcode::Mov, [asm_arg(r64(1)), asm_arg(r64(0))]);
                let (rc, rv) = (reg(S1, ty), reg(v, ty));
                let w = wide_ty(ty);
                let (wc, wv) = (reg(S1, w), reg(v, w));
                match op {
                    RmwOp::And => self.emit(Opcode::And, [asm_arg(wc), asm_arg(wv)]),
                    RmwOp::Or => self.emit(Opcode::Or, [asm_arg(wc), asm_arg(wv)]),
                    RmwOp::Xor => self.emit(Opcode::Xor, [asm_arg(wc), asm_arg(wv)]),
                    // Replace the old value by v where v wins, comparing at
                    // the type's width (`cmov` has no 8-bit form, but the
                    // low bits are what matter).
                    _ => {
                        let cc = match op {
                            RmwOp::Umin => "a",
                            RmwOp::Umax => "b",
                            RmwOp::Smin => "g",
                            _ => "l",
                        };
                        self.emit(Opcode::Cmp, [asm_arg(rc), asm_arg(rv)]);
                        self.emit(Opcode::Cmovcc(condition(cc)), [asm_arg(wc), asm_arg(wv)]);
                    }
                }
                self.emit_locked(Opcode::Cmpxchg, [asm_arg(at), asm_arg(rc)]);
                self.emit(Opcode::Jcc(5), [asm_arg(retry)]);
            }
        }
        if let Some(loc) = result.and_then(|r| self.defined(r)) {
            self.emit_move(loc, Loc::Reg(S0));
        }
    }

    fn switch(&mut self, inst: Inst, arg: Value, dests: &[BlockCall], cases: &[i64]) {
        let ty = self.ty(arg);
        // Compare the zero-extended bits with the cases as unsigned bits.
        let mask = mask(ty);
        let x = self.use_reg(arg, S0);
        let x = self.extend(S0, x, ty, false);
        let width = wide_ty(ty);
        let mut plan = SwitchPlan::new(cases, mask);
        let bounded = self
            .selection
            .switch_bounds
            .get(&inst)
            .is_some_and(|&range| plan.bound(range));
        let targets: Vec<String> = dests
            .iter()
            .enumerate()
            .map(|(i, d)| match self.block_args_noop(d, i) {
                true => self.label(d.block),
                false => format!(
                    ".L{}_{inst_name}_edge{i}",
                    self.func.name,
                    inst_name = self.func.inst_name(inst)
                ),
            })
            .collect();
        let (min, span) = (plan.min, plan.span);
        // A value that fits a sign-extended 32-bit immediate.
        let imm = |this: &mut Self, value: u64| -> AsmOperand {
            if width == Type::I64 && i32::try_from(value as i64).is_err() {
                this.emit(Opcode::Movabs, [asm_arg(r64(1)), asm_arg(value)]);
                asm_arg(r64(1))
            } else {
                asm_arg(value as i64)
            }
        };
        if plan.jump_table() {
            // idx = x - min, unsigned; anything above the span is default.
            // A 32-bit index is zero-extended by writing it.
            let rax = reg(S0, width);
            if min == 0 {
                self.emit(Opcode::Mov, [asm_arg(rax), asm_arg(reg(x, width))]);
            } else {
                let sub = imm(self, min);
                if x != S0 {
                    self.emit(Opcode::Mov, [asm_arg(rax), asm_arg(reg(x, width))]);
                }
                self.emit(Opcode::Sub, [asm_arg(rax), asm_arg(sub)]);
            }
            if !bounded {
                self.emit(Opcode::Cmp, [asm_arg(rax), asm_arg(span)]);
                self.emit(Opcode::Jcc(7), [asm_arg(&targets[0])]);
            }
            let table = format!(
                ".L{}_{inst_name}_table",
                self.func.name,
                inst_name = self.func.inst_name(inst)
            );
            // Offsets from the table to each target, so the table needs no
            // relocations: position independent.
            self.emit(
                Opcode::Lea,
                [
                    asm_arg(r64(1)),
                    asm_arg(symbol_memory(rip(), &table, Modifier::None)),
                ],
            );
            self.emit(
                Opcode::Movsxd,
                [
                    asm_arg(r64(0)),
                    asm_arg(sized(indexed(r64(1), r64(0), (4i128) as u8), 32)),
                ],
            );
            self.emit(Opcode::Add, [asm_arg(r64(0)), asm_arg(r64(1))]);
            self.emit(Opcode::Jmp, [asm_arg(r64(0))]);
            self.out.align(2);
            self.out.label(table.clone());
            for i in plan.table() {
                self.out
                    .integer(4, Expression::difference(targets[i].clone(), table.clone()));
            }
        } else {
            for (i, &value) in plan.values.iter().enumerate() {
                let operand = imm(self, value);
                self.emit(Opcode::Cmp, [asm_arg(reg(x, width)), asm_arg(operand)]);
                self.emit(Opcode::Jcc(4), [asm_arg(&targets[i + 1])]);
            }
            // Case landing pads are emitted before the next IR block. The
            // default may fall through only when none of those pads intervene.
            if dests
                .iter()
                .enumerate()
                .all(|(i, dest)| self.block_args_noop(dest, i))
            {
                self.branch_unless_next(dests[0].block);
            } else {
                self.emit(Opcode::Jmp, [asm_arg(&targets[0])]);
            }
        }
        for (i, (dest, target)) in dests.iter().zip(&targets).enumerate() {
            if !self.block_args_noop(dest, i) {
                self.out.label(target.clone());
                self.block_args(dest, i);
                self.jump_to_block(Opcode::Jmp, dest.block);
            }
        }
    }

    /// Emits the compare of `icmp`/`fcmp` `inst`, returning the condition
    /// code that holds when its condition does.
    ///
    /// `ucomisd a, b` sets ZF, PF and CF like an unsigned compare, and all
    /// three for unordered (NaN). So `a > b` is `a` (above: CF and ZF
    /// clear), false for NaN, and `a < b` is `b > a`. Equality needs two
    /// flags (ZF set, PF clear), combined into al and tested.
    fn compare(&mut self, inst: Inst) -> &'static str {
        if let Some(&(source, mask)) = self.selection.mask_tests.get(&inst) {
            let InstData::Icmp {
                cond: cond @ (IntCC::Eq | IntCC::Ne),
                ..
            } = *self.func.inst_data(inst)
            else {
                unreachable!("ZF-only mask test")
            };
            let a = self.use_reg(source, S0);
            self.emit(
                Opcode::Test,
                [asm_arg(reg(a, Type::I32)), asm_arg(mask as i32)],
            );
            return cond_code(cond);
        }
        if let Some(&(source, invert)) = self.selection.conditions.get(&inst) {
            let cc = self.compare(source);
            return if invert { invert_cond(cc) } else { cc };
        }
        match *self.func.inst_data(inst) {
            InstData::Icmp { cond, args } => {
                let ty = self.ty(args[0]);
                let a = self.use_reg(args[0], S0);
                if let Some(&addr) = self.selection.compare_memory.get(&inst) {
                    let at = self.mem(addr, S1, ty);
                    self.emit(Opcode::Cmp, [asm_arg(reg(a, ty)), asm_arg(&at)]);
                    return cond_code(cond);
                }
                let b = match folded_imm(self.func, self.func.inst_data(inst)) {
                    Some((_, k)) => asm_arg(k),
                    None => asm_arg(reg(self.use_reg(args[1], S1), ty)),
                };
                self.emit(Opcode::Cmp, [asm_arg(reg(a, ty)), asm_arg(b)]);
                cond_code(cond)
            }
            InstData::Fcmp { cond, args } => {
                let ty = self.ty(args[0]);
                if ty == Type::F80 {
                    return self.f80_compare(cond, args);
                }
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                let (xa, xb) = (xmm(a), xmm(b));
                let ucomi = scalar(Opcode::Ucomisd, ty);
                let (first, second, cc) = match cond {
                    FloatCC::Gt => (xa, xb, "a"),
                    FloatCC::Ge => (xa, xb, "ae"),
                    FloatCC::Lt => (xb, xa, "a"),
                    FloatCC::Le => (xb, xa, "ae"),
                    FloatCC::Ord => (xa, xb, "np"),
                    FloatCC::Uno => (xa, xb, "p"),
                    FloatCC::Eq | FloatCC::Ne => {
                        self.emit(ucomi, [asm_arg(xa), asm_arg(xb)]);
                        if self.func.resolve(args[0]) == self.func.resolve(args[1]) {
                            return if cond == FloatCC::Eq { "np" } else { "p" };
                        }
                        if cond == FloatCC::Eq {
                            self.emit(Opcode::Setcc(4), [asm_arg(r8(0))]);
                            self.emit(Opcode::Setcc(11), [asm_arg(r8(1))]);
                            self.emit(Opcode::And, [asm_arg(r8(0)), asm_arg(r8(1))]);
                        } else {
                            self.emit(Opcode::Setcc(5), [asm_arg(r8(0))]);
                            self.emit(Opcode::Setcc(10), [asm_arg(r8(1))]);
                            self.emit(Opcode::Or, [asm_arg(r8(0)), asm_arg(r8(1))]);
                        }
                        // AND/OR already set ZF from canonical al.
                        return "ne";
                    }
                };
                self.emit(ucomi, [asm_arg(first), asm_arg(second)]);
                cc
            }
            _ => unreachable!("not a compare"),
        }
    }

    /// Sets flags so that `ne` means `value` is non-zero, at its width; an
    /// `i128` is non-zero if either half is.
    fn test_value(&mut self, value: Value) {
        let ty = self.ty(value);
        if ty == Type::I128 {
            let [lo, hi] = self.halves(value);
            self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(lo)]);
            self.emit(Opcode::Or, [asm_arg(r64(0)), asm_arg(hi)]);
            return;
        }
        let c = self.use_reg(value, S0);
        let r = reg(c, ty);
        self.emit(Opcode::Test, [asm_arg(r), asm_arg(r)]);
    }

    /// Emits `moves`, each `(dst, src)`, as if they all happened at once.
    /// Integer copies break cycles through r11. SIMD copies use reserved
    /// xmm14/ymm14/zmm14, preserving every lane without reserving frame storage.
    /// emit_move uses only rax and xmm15/ymm15/zmm15 for memory transfers, so it
    /// cannot clobber either cycle temporary while draining a chain.
    fn parallel_copy(&mut self, moves: &[(Loc, Loc)]) {
        let simd = moves.iter().any(|&(dst, src)| {
            [dst, src].iter().any(|loc| match loc {
                Loc::FReg(_) => true,
                Loc::Stack(slot) => self.vector_spills.contains_key(slot),
                _ => false,
            })
        });
        let temp = if simd { Loc::FReg(FS1) } else { Loc::Reg(S3) };
        for_each_move(moves, temp, |dst, src| self.emit_move(dst, src));
    }

    /// Copy between allocated locations. Scalar slots carry 64 bits; vector
    /// slots and vector register copies preserve all 128 bits.
    fn emit_move(&mut self, dst: Loc, src: Loc) {
        use crate::backend_profile::{Counter, count};
        if crate::lowering::is_register_move(dst, src) {
            count(Counter::RegisterMoves);
        } else if dst != src {
            if matches!(src, Loc::Stack(_)) {
                count(Counter::Reloads);
            }
            if matches!(dst, Loc::Stack(_)) {
                count(Counter::SpillStores);
            }
        }
        match (dst, src) {
            (Loc::Remat(_), _) => unreachable!("rematerialized locations are source-only"),
            _ if dst == src => {}
            (Loc::Reg(d), Loc::Remat(value)) => self.rematerialize(d, value),
            (Loc::FReg(d), Loc::Remat(value)) => {
                self.rematerialize(S0, value);
                self.emit(Opcode::Movq, [asm_arg(xmm(d)), asm_arg(r64(S0))]);
            }
            (Loc::Stack(d), Loc::Remat(value)) => {
                // Match stack-to-stack copies: rax is reserved, while
                // rcx can already contain an outgoing argument. Keep the
                // parallel-copy cycle temporary (r11) intact.
                self.rematerialize(S0, value);
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized(self.slot(d), 64)), asm_arg(r64(S0))],
                );
            }
            (Loc::Reg(d), Loc::Reg(s)) => {
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(s))])
            }
            (Loc::FReg(d), Loc::FReg(s)) => self.emit(
                if self.wide_vectors {
                    Opcode::Vmovups
                } else {
                    Opcode::Movaps
                },
                [
                    asm_arg(simd_width(d, self.simd_bits)),
                    asm_arg(simd_width(s, self.simd_bits)),
                ],
            ),
            (Loc::Reg(d), Loc::FReg(s)) => {
                self.emit(Opcode::Movq, [asm_arg(r64(d)), asm_arg(xmm(s))])
            }
            (Loc::FReg(d), Loc::Reg(s)) => {
                self.emit(Opcode::Movq, [asm_arg(xmm(d)), asm_arg(r64(s))])
            }
            (Loc::Reg(d), Loc::Stack(s)) => self.emit(
                Opcode::Mov,
                [asm_arg(r64(d)), asm_arg(sized(self.slot(s), 64))],
            ),
            (Loc::FReg(d), Loc::Stack(s)) => {
                let bits = self.vector_spills.get(&s).copied().unwrap_or(64);
                self.emit(
                    move_bits(bits),
                    [asm_arg(simd_width(d, bits)), asm_arg(self.copy_slot(s))],
                );
            }
            (Loc::Stack(d), Loc::Reg(s)) => self.emit(
                Opcode::Mov,
                [asm_arg(sized(self.slot(d), 64)), asm_arg(r64(s))],
            ),
            (Loc::Stack(d), Loc::FReg(s)) => {
                let bits = self.vector_spills.get(&d).copied().unwrap_or(64);
                self.emit(
                    move_bits(bits),
                    [asm_arg(self.copy_slot(d)), asm_arg(simd_width(s, bits))],
                );
            }
            (Loc::Stack(d), Loc::Stack(s)) if self.vector_spills.contains_key(&d) => {
                let bits = self.vector_spills[&d];
                let source_bits = self.vector_spills.get(&s).copied().unwrap_or(64);
                self.emit(
                    move_bits(source_bits),
                    [
                        asm_arg(simd_width(FS2, source_bits)),
                        asm_arg(self.copy_slot(s)),
                    ],
                );
                self.emit(
                    move_bits(bits),
                    [asm_arg(self.copy_slot(d)), asm_arg(simd_width(FS2, bits))],
                );
            }
            (Loc::Stack(d), Loc::Stack(s)) => {
                self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(0)), asm_arg(sized(self.slot(s), 64))],
                );
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized(self.slot(d), 64)), asm_arg(r64(0))],
                );
            }
        }
    }

    /// A jump (`jmp` or `jcc`) to `block`.
    fn jump_to_block(&mut self, opcode: Opcode, block: Block) {
        let function = &self.func.name;
        self.out
            .jump_to_block(opcode, function, self.bare_name, block);
    }

    fn branch_unless_next(&mut self, target: Block) {
        if self.next_block != Some(target) {
            self.jump_to_block(Opcode::Jmp, target);
        }
    }

    fn zero_extended_narrow(&self, value: Value) -> bool {
        crate::fold::zero_extended_narrow(self.func, &self.selection, &self.alloc, value, false)
    }

    /// An i32 whose producer wrote its full register with a 32-bit ALU
    /// instruction or load, which clears the upper 32 bits. Every scalar
    /// register and spill copy moves 64 bits, so the zeros survive to each
    /// use. Parameters and call results come from other code, whose upper
    /// bits the ABI leaves undefined.
    fn zero_extended_i32(&self, value: Value) -> bool {
        use crate::regalloc::{OperandKind, ValuePart};
        let value = self.func.resolve(value);
        if self.func.value_type(value) != Type::I32 {
            return false;
        }
        let ValueDef::Result(inst, 0) = self.func.value_def(value) else {
            return false;
        };
        if self.selection.covered(inst) || self.selection.matches.contains_key(&inst) {
            return false;
        }
        if !matches!(
            self.alloc.operand_loc(
                self.alloc.edits.instruction_index(inst),
                ValuePart::low(value),
                OperandKind::Def
            ),
            Some(Loc::Reg(_) | Loc::Stack(_))
        ) {
            return false;
        }
        match *self.func.inst_data(inst) {
            InstData::Load { ty, .. } => ty == Type::I32,
            InstData::Binary { op, args } => {
                self.ty(args[0]) == Type::I32
                    && matches!(
                        op,
                        BinaryOp::Iadd
                            | BinaryOp::Isub
                            | BinaryOp::And
                            | BinaryOp::Or
                            | BinaryOp::Xor
                            | BinaryOp::Shl
                            | BinaryOp::Ushr
                            | BinaryOp::Sshr
                    )
                    && !self.selection.division_remainders.contains_key(&inst)
            }
            _ => false,
        }
    }

    /// Sign- or zero-extends a narrow value in `src` to 32 bits in
    /// `scratch` and returns `scratch`; returns `src` unchanged for 32-
    /// and 64-bit types. Extending in place would clobber the operand's
    /// register.
    fn extend(&mut self, scratch: u8, src: u8, ty: Type, signed: bool) -> u8 {
        if narrow_bits(ty).is_none() {
            return src;
        }
        let op = if signed { Opcode::Movsx } else { Opcode::Movzx };
        self.emit(op, [asm_arg(r32(scratch)), asm_arg(reg(src, ty))]);
        scratch
    }

    /// Loads constant `imm` of type `ty`: a 32-bit `mov` zero-extends, a
    /// 64-bit one sign-extends its 32-bit immediate, and `movabs` takes
    /// any 64 bits.
    fn materialize(&mut self, d: u8, imm: i64, ty: Type) {
        let bits = imm as u64 & mask(ty);
        if bits == 0 {
            self.emit(Opcode::Xor, [asm_arg(r32(d)), asm_arg(r32(d))]);
        } else {
            self.materialize_preserving_flags(d, bits);
        }
    }

    /// Materialize all 64 bits without disturbing a pending comparison.
    fn materialize_preserving_flags(&mut self, d: u8, bits: u64) {
        if bits <= u64::from(u32::MAX) {
            self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(bits)]);
        } else if i32::try_from(bits as i64).is_ok() {
            self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(bits as i64)]);
        } else {
            // MOV r32 zero-extends; NOT r64 reconstructs a high-ones mask.
            // Both instructions preserve flags. Compare concrete encodings
            // after allocation, including any extended-register prefixes.
            if self.goal == crate::CodegenGoal::Size && u32::try_from(!bits).is_ok() {
                let size = |opcode, operands| {
                    keel_asm::x86::instruction_size_with_features(
                        &keel_asm_ir::x86::Instruction {
                            opcode,
                            operands,
                            lock: false,
                        },
                        self.features,
                    )
                    .ok()
                    .flatten()
                };
                let full = size(Opcode::Movabs, vec![asm_arg(r64(d)), asm_arg(bits)]);
                let short = size(Opcode::Mov, vec![asm_arg(r32(d)), asm_arg(!bits)])
                    .zip(size(Opcode::Not, vec![asm_arg(r64(d))]))
                    .map(|(load, invert)| load + invert);
                // Unknown or rejected encodings retain the original lowering.
                if matches!((short, full), (Some(short), Some(full)) if short < full) {
                    self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(!bits)]);
                    self.emit(Opcode::Not, [asm_arg(r64(d))]);
                    return;
                }
            }
            self.emit(Opcode::Movabs, [asm_arg(r64(d)), asm_arg(bits)]);
        }
    }

    /// Recreates a pure value without changing flags: some lowering
    /// sequences load operands between a comparison and its conditional move.
    fn rematerialize(&mut self, d: u8, value: Value) {
        use crate::regalloc::{RematRecipe, remat_recipe};
        match remat_recipe(self.func, value).expect("valid rematerialization recipe") {
            RematRecipe::Iconst(bits) => {
                crate::backend_profile::count(crate::backend_profile::Counter::RematConstants);
                self.materialize_preserving_flags(d, bits);
            }
            RematRecipe::NullPtr => {
                crate::backend_profile::count(crate::backend_profile::Counter::RematConstants);
                self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(0u32)]);
            }
            RematRecipe::StackAddr(slot) => {
                crate::backend_profile::count(crate::backend_profile::Counter::RematStackAddresses);
                let at = self.frame(self.stack_slot_offsets[slot.index()]);
                self.emit(Opcode::Lea, [asm_arg(r64(d)), asm_arg(&at)]);
            }
        }
    }

    fn trap_label(&mut self) -> String {
        self.uses_trap = true;
        format!(".L{}_trap", self.func.name)
    }

    /// Resolve every address leaf before the destination write, including
    /// an address/result register alias. Complete the ordinary i32 spill home.
    fn signed_byte_load(&mut self, addr: Value, result: Option<Value>) {
        let at = self.mem(addr, S0, Type::I8);
        let d = self.def_reg(result);
        self.emit(Opcode::Movsx, [asm_arg(r32(d)), asm_arg(at)]);
        self.def_done(result, d);
    }

    /// The memory operand for an access of type `ty` at `addr`: the stack
    /// slot itself if `addr` is one's address, else the address in a
    /// register.
    fn mem(&mut self, addr: Value, scratch: u8, ty: Type) -> Memory {
        if let Some(slot) = stack_slot(self.func, addr) {
            return sized(self.frame(self.stack_slot_offsets[slot]), ptr(ty));
        }
        if let Some(&address) = self
            .selection
            .indexed_addresses
            .get(&self.func.resolve(addr))
        {
            // Store/CMP data and ALU accumulators stay in S0 or allocated
            // registers. S1/S2 are reserved, never allocation destinations,
            // and use_reg/rematerialize write only their requested scratch.
            // Keep reloads distinct even when both leaves spill. The memory
            // instruction reads both leaves before writing a load/ALU result,
            // so a coalesced lhs/destination may also supply an address leaf.
            let base = self.use_reg(address.base, S1);
            let index = self.use_reg(address.index, S2);
            return sized(
                indexed(r64(base), r64(index), 1u8 << address.shift),
                ptr(ty),
            );
        }
        if let Some(&(base, offset)) = self.selection.addresses.get(&self.func.resolve(addr)) {
            let base = self.use_reg(base, scratch);
            return sized(memory(r64(base), i128::from(offset)), ptr(ty));
        }
        sized(memory(r64(self.use_reg(addr, scratch)), 0), ptr(ty))
    }

    /// The register holding operand `value`: its own, or `scratch` after
    /// reloading it from its spill slot.
    fn use_reg(&mut self, value: Value, scratch: u8) -> u8 {
        match self.loc(value) {
            Loc::Reg(r) => r,
            Loc::Stack(s) => {
                crate::backend_profile::count(crate::backend_profile::Counter::Reloads);
                self.emit(Opcode::Mov, [asm_arg(r64(scratch)), asm_arg(self.slot(s))]);
                scratch
            }
            Loc::Remat(value) => {
                self.rematerialize(scratch, value);
                scratch
            }
            Loc::FReg(_) => unreachable!("{value} is a float"),
        }
    }

    /// [`Self::use_reg`] for floats.
    fn use_freg(&mut self, value: Value, scratch: u8) -> u8 {
        match self.loc(value) {
            Loc::FReg(r) => r,
            Loc::Stack(s) => {
                crate::backend_profile::count(crate::backend_profile::Counter::Reloads);
                let ty = self.ty(value);
                let bits = if ty.is_vector() { ptr(ty) } else { 64 };
                self.emit(
                    if ty.is_vector() {
                        movs(ty)
                    } else {
                        Opcode::Movsd
                    },
                    [
                        asm_arg(simd(scratch, ty)),
                        asm_arg(sized(self.slot(s), bits)),
                    ],
                );
                scratch
            }
            Loc::Reg(_) | Loc::Remat(_) => unreachable!("{value} is not a float"),
        }
    }

    /// The register to compute `result` into: its own, or rax if it is
    /// spilled (or an alias, whose value is never read).
    fn def_reg(&self, result: Option<Value>) -> u8 {
        let value = result.expect("instruction has a result");
        match self.defined(value) {
            Some(Loc::Reg(r)) => r,
            _ => S0,
        }
    }

    /// [`Self::def_reg`] for floats: the result's register, or xmm13.
    fn def_freg(&self, result: Option<Value>) -> u8 {
        let value = result.expect("instruction has a result");
        match self.defined(value) {
            Some(Loc::FReg(r)) => r,
            _ => FS0,
        }
    }

    /// Stores a spilled result computed into `reg` by [`Self::def_reg`] or
    /// [`Self::def_freg`].
    fn def_done(&mut self, result: Option<Value>, reg: u8) {
        let Some(value) = result else { return };
        if let Some(Loc::Stack(s)) = self.defined(value) {
            crate::backend_profile::count(crate::backend_profile::Counter::SpillStores);
            let ty = self.func.value_type(value);
            let bits = if ty.is_vector() { ptr(ty) } else { 64 };
            if ty.is_float() || ty.is_vector() {
                self.emit(
                    if ty.is_vector() {
                        movs(ty)
                    } else {
                        Opcode::Movsd
                    },
                    [asm_arg(sized(self.slot(s), bits)), asm_arg(simd(reg, ty))],
                );
            } else {
                self.emit(Opcode::Mov, [asm_arg(self.slot(s)), asm_arg(r64(reg))]);
            }
        }
    }

    /// Defines `result` from FS0, the scratch register an intrinsic lowering
    /// computes into, copying it when the allocator chose another register.
    fn def_vector_from_fs0(&mut self, result: Option<Value>, ty: Type) {
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                movs(ty),
                [asm_arg(simd(destination, ty)), asm_arg(simd(FS0, ty))],
            );
        }
        self.def_done(result, destination);
    }

    fn ty(&self, value: Value) -> Type {
        self.func.value_type(self.func.resolve(value))
    }

    /// The fixed frame at `offset` from `rsp` after the prologue, from rbp,
    /// which stays put if `rsp` moves.
    fn frame(&self, offset: usize) -> Memory {
        if let Some(r) = self.frame_register {
            return memory(r64(r), offset as i128);
        }
        let top = self.frame_size + 8 * self.saved.len();
        memory(r64(5), -((top - offset) as i128))
    }

    /// Spill address with the scalar transport width. A home may hold
    /// different types at different times; copy capacity is not its type.
    fn slot(&self, index: u32) -> Memory {
        sized(self.frame(self.spill_base + 8 * index as usize), 64)
    }

    /// Full-payload width for untyped transfers and dedicated copy staging.
    fn copy_slot(&self, index: u32) -> Memory {
        sized(
            self.slot(index),
            self.vector_spills.get(&index).copied().unwrap_or(64),
        )
    }

    fn label(&self, block: Block) -> String {
        crate::text::block_label(".L", &self.func.name, block)
    }

    fn emit<const N: usize>(&mut self, opcode: Opcode, operands: [AsmOperand; N]) {
        self.emit_instruction(opcode, operands, false);
    }
    fn emit_locked<const N: usize>(&mut self, opcode: Opcode, operands: [AsmOperand; N]) {
        self.emit_instruction(opcode, operands, true);
    }
    fn emit_instruction<const N: usize>(
        &mut self,
        opcode: Opcode,
        operands: [AsmOperand; N],
        lock: bool,
    ) {
        // Keep the legality guard in release builds and on the text path too.
        // AVX forms can require AVX2 depending on the borrowed operands.
        if self.feature_error.is_none() {
            let required = keel_asm_ir::x86::required_features(opcode, &operands);
            // Most instructions require nothing. Comparing with NONE costs
            // far less than `contains`, which packs both sets into bits.
            if required != keel_asm_ir::x86::Features::NONE && !self.features.contains(required) {
                self.feature_error = Some(format!(
                    "instruction {opcode:?} requires target features: {}",
                    required.names().collect::<Vec<_>>().join(", ")
                ));
            }
        }
        self.out.instruction(opcode, operands, lock);
    }
}

/// The second operand of a two-address instruction.
#[derive(Copy, Clone)]
enum Operand {
    Reg(u8),
    Imm(i64),
}

/// The convention's view of a function's own parameters.
fn param_kinds(sig: &keel_ir::Signature) -> Vec<ArgKind<'_>> {
    sig.params
        .iter()
        .map(|p| sysv::signature_param_kind(sig, p))
        .collect()
}

/// The convention's view of a call's arguments: the parameters `params`,
/// then any variadic arguments by their types.
fn arg_kinds<'s>(
    func: &Function,
    sig: &'s keel_ir::Signature,
    args: &'s keel_ir::CallArgs,
) -> Vec<ArgKind<'s>> {
    (args.iter().enumerate())
        .map(|(i, &arg)| arg_kind(func, sig, args, i, arg))
        .collect()
}

/// How argument `i`, `arg`, of a call with `params` crosses it.
fn arg_kind<'s>(
    func: &Function,
    sig: &'s keel_ir::Signature,
    args: &'s keel_ir::CallArgs,
    i: usize,
    arg: Value,
) -> ArgKind<'s> {
    if sig.x86_vector_abi.is_some()
        && func.value_type(func.resolve(arg)) == Type::Vector(keel_ir::VectorType::F64x1)
    {
        return ArgKind::StackValue(Type::Vector(keel_ir::VectorType::F64x1));
    }
    if i >= sig.params.len()
        && let Some(p) = args.abi(i)
        && let Purpose::ByVal(layout) = &p.purpose
    {
        if sysv::register_vector(layout).is_some_and(|t| t.bytes().is_some_and(|bytes| bytes >= 32))
        {
            return ArgKind::MemoryByVal(layout);
        }
        return sysv::signature_param_kind(sig, p);
    }
    match args.abi(i).or_else(|| sig.params.get(i)) {
        Some(p) => sysv::signature_param_kind(sig, p),
        None => {
            let ty = func.value_type(func.resolve(arg));
            if ty.bytes().is_some_and(|bytes| bytes >= 32) {
                ArgKind::StackValue(ty)
            } else {
                ArgKind::Value(ty)
            }
        }
    }
}

fn gpr(n: u8, width: u16) -> Register {
    Register {
        class: RegisterClass::Gpr,
        number: n,
        width,
        high_byte: false,
    }
}
fn r64(n: u8) -> Register {
    gpr(n, 64)
}
fn r32(n: u8) -> Register {
    gpr(n, 32)
}
fn r16(n: u8) -> Register {
    gpr(n, 16)
}
fn r8(n: u8) -> Register {
    gpr(n, 8)
}
fn reg(n: u8, ty: Type) -> Register {
    gpr(n, ptr(ty))
}

/// The width integer arithmetic on `ty` runs at: 64 bits for 64-bit
/// types, else 32.
fn wide_ty(ty: Type) -> Type {
    match ty {
        Type::I64 | Type::Ptr => Type::I64,
        _ => Type::I32,
    }
}

/// Register `n` at the width arithmetic on `ty` runs at.
fn wide(n: u8, ty: Type) -> Register {
    reg(n, wide_ty(ty))
}

fn simd_width(n: u8, bits: u16) -> Register {
    if bits >= 256 {
        Register {
            class: if bits == 512 {
                RegisterClass::Zmm
            } else {
                RegisterClass::Ymm
            },
            number: n,
            width: bits,
            high_byte: false,
        }
    } else {
        xmm(n)
    }
}
fn simd(n: u8, ty: Type) -> Register {
    simd_width(n, ptr(ty))
}
fn xmm(n: u8) -> Register {
    Register {
        class: RegisterClass::Xmm,
        number: n,
        width: 128,
        high_byte: false,
    }
}

/// The size keyword of a memory operand of type `ty`.
fn ptr(ty: Type) -> u16 {
    match ty {
        Type::Vector(v) => (v.bytes() * 8) as u16,
        Type::I8 => 8,
        Type::I16 => 16,
        Type::F80 => 80,
        Type::I32 | Type::F32 => 32,
        _ => 64,
    }
}

/// The scalar float or unaligned vector move.
fn movs(ty: Type) -> Opcode {
    if let Type::Vector(v) = ty {
        return match v.bytes() {
            8 => Opcode::Movq,
            16 => Opcode::Movups,
            32 | 64 => Opcode::Vmovups,
            _ => unreachable!(),
        };
    }
    if ty == Type::F32 {
        Opcode::Movss
    } else {
        Opcode::Movsd
    }
}

fn narrow_bits(ty: Type) -> Option<u32> {
    match ty {
        Type::I8 => Some(8),
        Type::I16 => Some(16),
        _ => None,
    }
}

/// The bits of a value of integer type `ty`.
fn mask(ty: Type) -> u64 {
    u64::MAX >> (64 - 8 * ty.bytes().unwrap_or(8).min(8))
}

/// The most negative value of `ty`, sign-extended to 64 bits.
fn min_value(ty: Type) -> u64 {
    let bits = 8 * ty.bytes().unwrap_or(8);
    u64::MAX << (bits - 1)
}

fn cond_code(cond: IntCC) -> &'static str {
    match cond {
        IntCC::Eq => "e",
        IntCC::Ne => "ne",
        IntCC::Slt => "l",
        IntCC::Sle => "le",
        IntCC::Sgt => "g",
        IntCC::Sge => "ge",
        IntCC::Ult => "b",
        IntCC::Ule => "be",
        IntCC::Ugt => "a",
        IntCC::Uge => "ae",
    }
}

/// The condition code that holds when `cond` does not.
fn invert_cond(cond: &str) -> &'static str {
    match cond {
        "e" => "ne",
        "ne" => "e",
        "l" => "ge",
        "ge" => "l",
        "le" => "g",
        "g" => "le",
        "b" => "ae",
        "ae" => "b",
        "be" => "a",
        "a" => "be",
        "p" => "np",
        "np" => "p",
        _ => panic!("unknown condition `{cond}`"),
    }
}

fn dwarf_register(r: u8) -> u8 {
    [0, 2, 1, 3, 7, 6, 4, 5, 8, 9, 10, 11, 12, 13, 14, 15][usize::from(r)]
}

fn condition(cc: &str) -> u8 {
    match cc {
        "o" => 0,
        "no" => 1,
        "b" | "c" => 2,
        "ae" | "nc" => 3,
        "e" => 4,
        "ne" => 5,
        "be" => 6,
        "a" => 7,
        "s" => 8,
        "ns" => 9,
        "p" => 10,
        "np" => 11,
        "l" => 12,
        "ge" => 13,
        "le" => 14,
        "g" => 15,
        _ => unreachable!("compiler condition {cc}"),
    }
}

fn scalar(op: Opcode, ty: Type) -> Opcode {
    if ty != Type::F32 {
        return match op {
            Opcode::Add => Opcode::Addsd,
            Opcode::Sub => Opcode::Subsd,
            Opcode::Mul => Opcode::Mulsd,
            Opcode::Div => Opcode::Divsd,
            other => other,
        };
    }
    match op {
        Opcode::Add | Opcode::Addsd => Opcode::Addss,
        Opcode::Sub | Opcode::Subsd => Opcode::Subss,
        Opcode::Mul => Opcode::Mulss,
        Opcode::Div => Opcode::Divss,
        Opcode::Sqrtsd => Opcode::Sqrtss,
        Opcode::Roundsd => Opcode::Roundss,
        Opcode::Ucomisd => Opcode::Ucomiss,
        Opcode::Minsd => Opcode::Minss,
        Opcode::Maxsd => Opcode::Maxss,
        Opcode::Cvtsi2sd => Opcode::Cvtsi2ss,
        Opcode::Cvttsd2si => Opcode::Cvttss2si,
        _ => unreachable!("compiler scalar opcode"),
    }
}
fn packed(op: Opcode, ty: Type) -> Opcode {
    match (op, ty == Type::F32) {
        (Opcode::Or, true) => Opcode::Orps,
        (Opcode::Or, false) => Opcode::Orpd,
        (Opcode::And, true) => Opcode::Andps,
        (Opcode::And, false) => Opcode::Andpd,
        _ => unreachable!("compiler packed opcode"),
    }
}

fn move_bits(bits: u16) -> Opcode {
    match bits {
        256 | 512 => Opcode::Vmovups,
        128 => Opcode::Movups,
        _ => Opcode::Movsd,
    }
}

#[cfg(test)]
mod bounded_unnamed_selection_tests {
    use super::*;
    #[test]
    fn huge_unnamed_memory_operand_selects_without_copy_or_frame_emission() {
        for producer in ["gnu", "clang"] {
            for indirect in [false, true] {
                let call = if indirect {
                    format!(
                        "call_indirect v1(v0 : ptr byval [100000000 x f64x1]) : (...) x86_vector_abi({producer})"
                    )
                } else {
                    "call @sink(v0 : ptr byval [100000000 x f64x1])".to_owned()
                };
                let module = keel_ir::parse(&format!(
                    "decl @sink(...) x86_vector_abi({producer})\nfunc @send(ptr,ptr) {{\nentry(v0:ptr,v1:ptr):\n {call}\n return\n}}"
                )).unwrap();
                keel_ir::verify_module(&module).unwrap();
                let func = &module.functions[0];
                let mut selected = 0;
                for (_, inst) in func.layout_insts() {
                    let (sig, args) = match func.inst_data(inst) {
                        InstData::Call { func: callee, args } => (&func.callee(*callee).sig, args),
                        InstData::CallIndirect { sig, args, .. } => (func.signature(*sig), args),
                        _ => continue,
                    };
                    let kinds = arg_kinds(func, sig, args);
                    assert_eq!(kinds.len(), 1);
                    let plan = sysv::arg_locs(&kinds);
                    assert_eq!(plan.locs, vec![ArgLoc::OnStack(0)]);
                    assert_eq!(plan.stack, 800_000_000);
                    selected += 1;
                }
                assert_eq!(selected, 1);
            }
        }
    }
}

#[cfg(test)]
mod feature_guard_tests {
    use super::*;
    use keel_asm_ir::x86::{CpuLevel, Features, Instruction};

    #[test]
    fn emission_reports_illegal_features_in_text_and_structured_modes() {
        let module = keel_ir::parse("func @f() {\nblock0:\n return\n}").unwrap();
        let symbols = crate::symbols::Symbols::new(&module);
        for (features, source, missing) in [
            (
                Features::for_level(CpuLevel::V3),
                "pclmulqdq xmm0,xmm1,255",
                "pclmulqdq",
            ),
            (Features::for_level(CpuLevel::V3), "aesenc xmm0,xmm1", "aes"),
            (
                Features::for_level(CpuLevel::V1),
                "popcnt rax,rax",
                "popcnt",
            ),
            (
                Features::for_level(CpuLevel::V1),
                "roundsd xmm0,xmm0,8",
                "sse41",
            ),
            (
                Features::for_level(CpuLevel::V2),
                "vaddps xmm0,xmm1,xmm2",
                "avx",
            ),
            (
                Features::for_level(CpuLevel::V2),
                "vpaddd ymm0,ymm1,ymm2",
                "avx2",
            ),
            (
                Features::for_level(CpuLevel::V2),
                "andn rax,rcx,rdx",
                "bmi1",
            ),
            (
                Features::for_level(CpuLevel::V2),
                "pext rax,rcx,rdx",
                "bmi2",
            ),
            (
                Features::for_level(CpuLevel::V2),
                "vfmadd132ps xmm0,xmm1,xmm2",
                "fma",
            ),
            (
                Features::for_level(CpuLevel::V2),
                "vcvtph2ps xmm0,xmm1",
                "f16c",
            ),
            (Features::for_level(CpuLevel::V2), "lzcnt rax,rcx", "lzcnt"),
            (
                Features::for_level(CpuLevel::V2),
                "movbe rax,qword ptr [rcx]",
                "movbe",
            ),
            (
                Features::for_level(CpuLevel::V2),
                "clflush [rax]",
                "clflush",
            ),
            // Memory broadcasts need AVX; register broadcasts need AVX2.
            (
                Features::for_level(CpuLevel::V1)
                    .with_features("avx")
                    .unwrap(),
                "vbroadcastss ymm0,xmm1",
                "avx2",
            ),
        ] {
            for structured in [false, true] {
                let selector = crate::x86_features::FeatureSelector {
                    inner: &crate::SelectionTier::Baseline,
                    features,
                };
                let pipeline = crate::CodegenPipeline {
                    selector: &selector,
                    allocator: &crate::regalloc::AllocationTier::Full,
                };
                let mut compiler = FunctionCompiler::new(
                    &module.functions[0],
                    &symbols,
                    &REGISTERS,
                    &pipeline,
                    structured,
                    crate::CodegenGoal::default(),
                    false,
                )
                .unwrap();
                let i = Instruction::parse_intel(source).unwrap();
                match i.operands.len() {
                    1 => compiler.emit(i.opcode, <[AsmOperand; 1]>::try_from(i.operands).unwrap()),
                    2 => compiler.emit(i.opcode, <[AsmOperand; 2]>::try_from(i.operands).unwrap()),
                    3 => compiler.emit(i.opcode, <[AsmOperand; 3]>::try_from(i.operands).unwrap()),
                    _ => unreachable!(),
                }
                // Start the function with an empty output as compile requires;
                // the diagnostic recorded by the emission guard must survive.
                compiler.out = Output::new(structured);
                let error = compiler.compile(&mut Output::new(structured)).unwrap_err();
                assert_eq!(error.func, "f");
                assert!(error.message.contains(missing), "{error:?}");
            }
        }
    }
}
