//! Non-emitting descriptors for a finite physical x86 stack/frame region.
//!
//! Structural acceptance does not prove memory disjointness, a legal machine
//! continuation, exception delivery, or native acceptance. No InstData, parser,
//! intrinsic, machine emitter, handler, or numerical stack oracle is provided.
//! In particular, arena I64 metadata cannot establish an exclusive memory borrow.
use crate::Type;

pub const INPUT_TYPES: [Type; 6] = [
    Type::Ptr, // exclusive arena base (unsafe caller precondition)
    Type::I64, // unsigned arena byte length, not a permission or bounds proof
    Type::I64, // actual raw RSP bit pattern
    Type::I64, // actual raw RBP bit pattern
    Type::I64, // initial full RAX
    Type::I64, // initial full RCX
];
pub const MAX_OPERATIONS: usize = 64;
pub const MAX_OBSERVATIONS: u8 = 16;

/// W32 represents a request that must be rejected in this long-mode stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    W16,
    W32,
    W64,
}
impl Width {
    fn bytes(self) -> Result<u8, &'static str> {
        match self {
            Self::W16 => Ok(2),
            Self::W64 => Ok(8),
            Self::W32 => Err("32-bit stack operand is not encodable in long mode"),
        }
    }
}

/// Explicit AnchorDx requests are representable for diagnostics, never legal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawReg {
    A,
    C,
    Sp,
    Bp,
    AnchorDx,
}
fn raw(reg: RawReg) -> Result<(), &'static str> {
    if reg == RawReg::AnchorDx {
        Err("RDX is the immutable compiler anchor, not a raw operand")
    } else {
        Ok(())
    }
}

/// Default 64-bit explicit address, no overrides/segments/relocations.
/// Wide fields deliberately allow checking invalid untrusted descriptors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mem64 {
    pub base: Option<RawReg>,
    pub index: Option<RawReg>,
    pub scale: u8,
    pub disp: i64,
}
impl Mem64 {
    fn check(self) -> Result<(), &'static str> {
        for reg in [self.base, self.index].into_iter().flatten() {
            raw(reg)?;
        }
        if self.index == Some(RawReg::Sp) {
            return Err("RSP cannot be an explicit memory index");
        }
        if !matches!(self.scale, 1 | 2 | 4 | 8) || (self.index.is_none() && self.scale != 1) {
            return Err("invalid explicit address scale");
        }
        if i32::try_from(self.disp).is_err() {
            return Err("explicit address displacement must fit signed32");
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushSource {
    Reg(RawReg),
    Mem(Mem64),
    /// Force imm8 encoding, architecturally sign-extended to operand width.
    Imm8(i8),
    /// Literal 16-bit encoding; legal only with W16.
    Imm16(u16),
    /// Force imm32 encoding, sign-extended to 64 bits; legal only with W64.
    Imm32(i32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopDest {
    Reg(RawReg),
    Mem(Mem64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Push {
        width: Width,
        src: PushSource,
    },
    Pop {
        width: Width,
        dst: PopDest,
    },
    Enter {
        width: Width,
        alloc: u16,
        nesting: u8,
    },
    Leave {
        width: Width,
    },
    /// Exact MOV m64,r64 observation; never a low16-only result.
    Observe {
        reg: RawReg,
        slot: u8,
    },
    /// Restore immutable compiler RBP and RSP identities before publication.
    Finish,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub operations: Vec<Operation>,
    pub observations: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obligation {
    /// ALWAYS unresolved: neither type checking nor arena metadata proves it.
    CallerArenaFootprint,
    CaptureAllocationAndPublication,
    ExactMachineExpansion,
    CompilerIdentityRestoration,
    CfaAndObjectRepresentation,
    LegalEntryAndNormalExit,
    /// Delivery must be independent of a possibly invalid raw RSP.
    IndependentStackTerminalMonitor,
    AsynchronousEnvironment,
}
pub const UNRESOLVED: [Obligation; 8] = [
    Obligation::CallerArenaFootprint,
    Obligation::CaptureAllocationAndPublication,
    Obligation::ExactMachineExpansion,
    Obligation::CompilerIdentityRestoration,
    Obligation::CfaAndObjectRepresentation,
    Obligation::LegalEntryAndNormalExit,
    Obligation::IndependentStackTerminalMonitor,
    Obligation::AsynchronousEnvironment,
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagsEffect {
    /// Architectural defined-FLAGS effect, not arbitrary RF/debug transparency.
    None,
}
/// Most recent frame update; must be read together with Enter16Dependencies.
/// In particular Partial16Write does not resolve inherited upper-BP uncertainty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKnowledge {
    CallerSeed,
    HardwareFrameTemp64,
    /// Intel093 ambiguity is not resolved by assigning a numerical RBP value.
    UnknownEnter16FrameTemp,
    LoadedFromStack,
    Partial16Write,
    CompilerEntryIdentity,
}
/// Structural register dependence on ENTER16's unresolved frame transfer.
/// False does NOT mean a register value, address, or memory footprint is known.
/// This tracks retained register bits/identity, not transitive memory-value taint:
/// a full stack load replaces the destination register's prior bits, but its
/// address/value/permission remain unproved under the eight unresolved duties.
/// Low16 writes preserve these markers conservatively; arithmetic on RSP does
/// too. LEAVE copies BP's marker to SP before its pop, including for LEAVE64.
/// ENTER64 replaces BP from SP, so it inherits SP's marker rather than blindly
/// clearing it. Finish requests restoration of independent compiler identities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Enter16Dependencies {
    pub bp: bool,
    pub sp: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackTransition {
    Unchanged,
    Push {
        bytes: u8,
    },
    Pop {
        bytes: u8,
    },
    PopIntoRsp,
    /// Write low16 of the incremented full RSP, preserving its upper bits.
    PopIntoSp,
    Enter {
        bytes: u8,
        pushes: u8,
        alloc: u16,
    },
    Leave {
        bytes: u8,
    },
    RestoreCompilerIdentity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressMoment {
    BeforeStackChange,
    AfterPopIncrement,
}
/// Ordered event templates, not concrete addresses or promises of fault priority.
/// A failed instruction's committed state must come from hardware context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryEvent {
    ExplicitRead {
        address: Mem64,
        bytes: u8,
        at: AddressMoment,
    },
    StackPush {
        bytes: u8,
    },
    StackRead {
        bytes: u8,
    },
    ExplicitWrite {
        address: Mem64,
        bytes: u8,
        at: AddressMoment,
    },
    /// Read old-frame display element after the indicated full-RBP decrement.
    DisplayRead {
        ordinal: u8,
        bytes: u8,
    },
    /// ENTER check is not a synthesized store or a sweep of local bytes.
    FinalSpWritePermissionCheck,
    ObservationHomeWrite64 {
        slot: u8,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterWrite {
    Full64(RawReg),
    Low16PreserveUpper(RawReg),
    /// ENTER16 frame transfer is intentionally not assigned either write model.
    UnknownEnter16Bp,
    RestoreCompilerRspAndRbp,
}
/// Retains the literal descriptor so an eventual emitter cannot silently choose
/// a different immediate encoding, address form, operand width or nesting byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectTemplate {
    pub intent: Operation,
    pub flags: FlagsEffect,
    pub register_reads: Vec<RawReg>,
    pub register_writes: Vec<RegisterWrite>,
    pub memory: Vec<MemoryEvent>,
    pub stack: StackTransition,
    pub frame_after: FrameKnowledge,
    /// Preserve provenance at the source of observations and memory operations.
    pub enter16_before: Enter16Dependencies,
    pub enter16_after: Enter16Dependencies,
    /// Every expansion boundary, not only a principal opcode, needs a monitor.
    pub terminal_boundary: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralReport {
    pub result_types: Vec<Type>,
    pub effects: Vec<EffectTemplate>,
    pub unresolved: [Obligation; 8],
    /// No memory safety inference follows from this bit (or from false).
    pub has_data_loaded_stack_or_frame: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanError {
    pub operation: Option<usize>,
    pub message: &'static str,
}
impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(i) = self.operation {
            write!(f, "stack region operation {i}: ")?;
        }
        f.write_str(self.message)
    }
}
impl std::error::Error for PlanError {}

fn write(reg: RawReg, bytes: u8) -> RegisterWrite {
    if bytes == 2 {
        RegisterWrite::Low16PreserveUpper(reg)
    } else {
        RegisterWrite::Full64(reg)
    }
}
fn address_reads(address: Mem64, reads: &mut Vec<RawReg>) {
    reads.extend([address.base, address.index].into_iter().flatten());
}

/// Analyze a straight-line descriptor only. All I64 inputs remain uninterpreted
/// bit patterns; no bounds/permission check, stack simulation or machine code is
/// produced. Finish is a requested future identity restore, not proof it occurs.
pub fn analyze(plan: &Plan, inputs: &[Type]) -> Result<StructuralReport, PlanError> {
    let fail = |operation, message| PlanError { operation, message };
    if inputs != INPUT_TYPES {
        return Err(fail(None, "expected exactly (ptr,i64,i64,i64,i64,i64)"));
    }
    if plan.operations.is_empty() || plan.operations.len() > MAX_OPERATIONS {
        return Err(fail(None, "expected 1..=64 total operations"));
    }
    if plan.observations > MAX_OBSERVATIONS {
        return Err(fail(None, "at most 16 full64 observations"));
    }
    let mut seen = 0u32;
    let mut finished = false;
    let mut frame = FrameKnowledge::CallerSeed;
    let mut enter16 = Enter16Dependencies::default();
    let mut effects = Vec::new();
    let mut data_loaded = false;
    for (i, &op) in plan.operations.iter().enumerate() {
        let error = |message| fail(Some(i), message);
        if finished {
            return Err(error("operation after sole finish"));
        }
        let mut effect = EffectTemplate {
            intent: op,
            flags: FlagsEffect::None,
            register_reads: vec![],
            register_writes: vec![],
            memory: vec![],
            stack: StackTransition::Unchanged,
            frame_after: frame,
            enter16_before: enter16,
            enter16_after: enter16,
            terminal_boundary: true,
        };
        match op {
            Operation::Push { width, src } => {
                let bytes = width.bytes().map_err(error)?;
                match src {
                    PushSource::Reg(reg) => {
                        raw(reg).map_err(error)?;
                        effect.register_reads.push(reg);
                    }
                    PushSource::Mem(address) => {
                        address.check().map_err(error)?;
                        address_reads(address, &mut effect.register_reads);
                        effect.memory.push(MemoryEvent::ExplicitRead {
                            address,
                            bytes,
                            at: AddressMoment::BeforeStackChange,
                        });
                    }
                    PushSource::Imm16(_) if bytes != 2 => {
                        return Err(error("imm16 encoding requires PUSH16"));
                    }
                    PushSource::Imm32(_) if bytes != 8 => {
                        return Err(error("imm32 encoding requires PUSH64"));
                    }
                    PushSource::Imm8(_) | PushSource::Imm16(_) | PushSource::Imm32(_) => {}
                }
                effect.register_reads.push(RawReg::Sp);
                effect
                    .register_writes
                    .push(RegisterWrite::Full64(RawReg::Sp));
                effect.memory.push(MemoryEvent::StackPush { bytes });
                effect.stack = StackTransition::Push { bytes };
            }
            Operation::Pop { width, dst } => {
                let bytes = width.bytes().map_err(error)?;
                effect.register_reads.push(RawReg::Sp);
                effect
                    .register_writes
                    .push(RegisterWrite::Full64(RawReg::Sp));
                effect.memory.push(MemoryEvent::StackRead { bytes });
                effect.stack = StackTransition::Pop { bytes };
                match dst {
                    PopDest::Reg(reg) => {
                        raw(reg).map_err(error)?;
                        effect.register_writes.push(write(reg, bytes));
                        if reg == RawReg::Sp {
                            if bytes == 8 {
                                enter16.sp = false;
                            }
                            data_loaded = true;
                            effect.stack = if bytes == 2 {
                                StackTransition::PopIntoSp
                            } else {
                                StackTransition::PopIntoRsp
                            };
                        }
                        if reg == RawReg::Bp {
                            if bytes == 8 {
                                enter16.bp = false;
                            }
                            data_loaded = true;
                            frame = if bytes == 2 {
                                FrameKnowledge::Partial16Write
                            } else {
                                FrameKnowledge::LoadedFromStack
                            };
                        }
                    }
                    PopDest::Mem(address) => {
                        address.check().map_err(error)?;
                        address_reads(address, &mut effect.register_reads);
                        effect.memory.push(MemoryEvent::ExplicitWrite {
                            address,
                            bytes,
                            at: AddressMoment::AfterPopIncrement,
                        });
                    }
                }
            }
            Operation::Enter {
                width,
                alloc,
                nesting,
            } => {
                let bytes = width.bytes().map_err(error)?;
                let level = nesting & 31;
                let pushes = if level == 0 { 1 } else { level + 1 };
                effect.register_reads.extend([RawReg::Sp, RawReg::Bp]);
                effect
                    .register_writes
                    .push(RegisterWrite::Full64(RawReg::Sp));
                effect.memory.push(MemoryEvent::StackPush { bytes });
                for ordinal in 1..level {
                    effect
                        .memory
                        .push(MemoryEvent::DisplayRead { ordinal, bytes });
                    effect.memory.push(MemoryEvent::StackPush { bytes });
                }
                if level != 0 {
                    effect.memory.push(MemoryEvent::StackPush { bytes });
                }
                effect.memory.push(MemoryEvent::FinalSpWritePermissionCheck);
                effect.stack = StackTransition::Enter {
                    bytes,
                    pushes,
                    alloc,
                };
                if bytes == 2 {
                    enter16.bp = true;
                    frame = FrameKnowledge::UnknownEnter16FrameTemp;
                    effect.register_writes.push(RegisterWrite::UnknownEnter16Bp);
                } else {
                    enter16.bp = enter16.sp;
                    frame = FrameKnowledge::HardwareFrameTemp64;
                    effect
                        .register_writes
                        .push(RegisterWrite::Full64(RawReg::Bp));
                }
            }
            Operation::Leave { width } => {
                let bytes = width.bytes().map_err(error)?;
                effect.register_reads.push(RawReg::Bp);
                effect
                    .register_writes
                    .extend([RegisterWrite::Full64(RawReg::Sp), write(RawReg::Bp, bytes)]);
                effect.memory.push(MemoryEvent::StackRead { bytes });
                effect.stack = StackTransition::Leave { bytes };
                enter16.sp = enter16.bp;
                if bytes == 8 {
                    enter16.bp = false;
                }
                frame = if bytes == 2 {
                    FrameKnowledge::Partial16Write
                } else {
                    FrameKnowledge::LoadedFromStack
                };
                data_loaded = true;
            }
            Operation::Observe { reg, slot } => {
                raw(reg).map_err(error)?;
                if slot >= plan.observations {
                    return Err(error("observation index out of range"));
                }
                let bit = 1u32 << slot;
                if seen & bit != 0 {
                    return Err(error("observation defined more than once"));
                }
                seen |= bit;
                effect.register_reads.push(reg);
                effect
                    .memory
                    .push(MemoryEvent::ObservationHomeWrite64 { slot });
            }
            Operation::Finish => {
                finished = true;
                enter16 = Enter16Dependencies::default();
                frame = FrameKnowledge::CompilerEntryIdentity;
                effect.stack = StackTransition::RestoreCompilerIdentity;
                effect
                    .register_writes
                    .push(RegisterWrite::RestoreCompilerRspAndRbp);
            }
        }
        effect.frame_after = frame;
        effect.enter16_after = enter16;
        effects.push(effect);
    }
    if !finished {
        return Err(fail(None, "missing sole finish"));
    }
    if seen != (1u32 << plan.observations) - 1 {
        return Err(fail(None, "undefined observation"));
    }
    Ok(StructuralReport {
        result_types: vec![Type::I64; plan.observations as usize],
        effects,
        unresolved: UNRESOLVED,
        has_data_loaded_stack_or_frame: data_loaded,
    })
}
