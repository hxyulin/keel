//! Structural verifier and raw program for the terminal-monitor FLAGS IR wrapper.
//!
//! The X86RflagsRegion wrapper binds this plan to I64 operands/results. Structural
//! success is not a native safety certificate and never unblocks standalone ID 730.
//! Codegen additionally checks its exact post-allocation expansion; environment
//! preconditions belong to the explicit terminal external monitor contract.
use crate::Type;

pub const MAX_BLOCKS: usize = 64;
pub const MAX_OPERATIONS: usize = 256;
pub const MAX_CAPTURES: usize = 16;
pub const MAX_OBSERVATIONS: usize = 16;

/// All values are captured before SaveEntry; only exact I64 images are accepted.
/// Observations remain private until normal exit after entry-image reapplication.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Plan {
    pub captures: Vec<Type>,
    pub observations: usize,
    /// Block zero is the sole entry. Edges must go forward: no hidden loop bound.
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Block {
    pub operations: Vec<Operation>,
    pub exit: Exit,
}

/// Closed raw-program subset. There is no ordinary integer/SIMD/memory lowering,
/// call, inline asm, stack adjustment, user-supplied opcode, or CFG escape here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Operation {
    /// Exact PUSHFQ image is kept in a private immutable restore capability.
    /// This starts ownership; it does not capture function-entry FLAGS.
    SaveEntry,
    /// Full hardware contract of existing intrinsic 730 inside the owned wrapper.
    /// Input capture precedes all ownership; old normalized image is observed.
    Swap { capture: usize, observation: usize },
    /// Actual PUSHFQ image at this program point, not numerical flag packing.
    Read { observation: usize },
    /// Apply the hidden saved image with actual POPFQ semantics. This is NOT
    /// proof of equality to entry RFLAGS or of a legal normal ABI execution state.
    ReapplyEntryImage,
}

/// Exact Jcc condition, encoded as its architectural low four opcode bits.
/// Named variants prevent accepting arbitrary branch-generating arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Condition {
    Overflow,
    NotOverflow,
    Below,
    AboveOrEqual,
    Equal,
    NotEqual,
    BelowOrEqual,
    Above,
    Sign,
    NotSign,
    Parity,
    NotParity,
    Less,
    GreaterOrEqual,
    LessOrEqual,
    Greater,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Exit {
    Jump(usize),
    /// Jcc directly reads the owned current machine FLAGS; no CMP/TEST is added.
    FlagsBranch {
        condition: Condition,
        taken: usize,
        not_taken: usize,
    },
    /// Transfer back to the wrapper, which may publish results and resume ABI IR
    /// only after ALL non-structural obligations are independently established.
    Normal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Obligation {
    /// CPL/IOPL, IF, DF, TF, AC/SMAP, RF/VM normalization, NT, VIF/VIP, reserved
    /// bits, and debug configuration are still architectural environment state.
    LegalEntryAndNormalExit,
    /// Every input/alias is captured before SaveEntry; register and spill slots
    /// cannot change after POPFQ until entry-image reapplication completes.
    CaptureAllocationAndPublication,
    /// Exact instruction expansion, resource uses/defs, RSP deltas, fault PCs,
    /// CFI and optimization/scheduling exclusion must match the analyzed plan.
    ExactMachineExpansion,
    /// TF traps, stack/page/alignment faults and other exceptional exits require
    /// an external terminal monitor; no handler resume or implicit cleanup proof.
    TerminalExceptionBoundary,
    /// IF changes allow interruption; no arbitrary signal/interrupt transparency
    /// follows from a normal CFG ownership proof.
    AsynchronousEnvironment,
}

/// Structural analysis deliberately has no `ready`, `safe`, or `verified` bit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralReport {
    pub blocks: usize,
    pub operations: usize,
    pub normal_exits: usize,
    pub unresolved: [Obligation; 5],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanError {
    pub block: Option<usize>,
    pub message: &'static str,
}
impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(block) = self.block {
            write!(f, "FLAGS region block {block}: ")?;
        }
        f.write_str(self.message)
    }
}
impl std::error::Error for PlanError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Phase {
    Outside,
    Owned,
    ImageReapplied,
}
#[derive(Clone, Copy)]
struct Flow {
    phase: Phase,
    must_observe: u16,
    may_observe: u16,
}

/// Check bounded CFG shape and linear ownership, with an implicit token phi at
/// each join. Incoming tokens must have the same ownership phase. On a branch
/// only the taken successor receives the token; it is not duplicated at runtime.
/// All possible normal paths must reapply the immutable entry image and define
/// every observation. Exceptional machine edges remain explicit obligations.
pub fn analyze(plan: &Plan) -> Result<StructuralReport, PlanError> {
    let error = |block, message| PlanError { block, message };
    if plan.blocks.is_empty() || plan.blocks.len() > MAX_BLOCKS {
        return Err(error(None, "expected 1..=64 blocks"));
    }
    if plan.captures.len() > MAX_CAPTURES || plan.captures.iter().any(|&t| t != Type::I64) {
        return Err(error(None, "captures must be at most 16 exact I64 images"));
    }
    if plan.observations > MAX_OBSERVATIONS {
        return Err(error(None, "at most 16 observations are supported"));
    }
    let operations = plan
        .blocks
        .iter()
        .map(|b| b.operations.len())
        .sum::<usize>();
    if operations > MAX_OPERATIONS {
        return Err(error(None, "at most 256 raw operations are supported"));
    }
    if plan.blocks[0].operations.first() != Some(&Operation::SaveEntry) {
        return Err(error(Some(0), "sole entry must begin with SaveEntry"));
    }
    let required = ((1u32 << plan.observations) - 1) as u16;
    let mut incoming = vec![None; plan.blocks.len()];
    incoming[0] = Some(Flow {
        phase: Phase::Outside,
        must_observe: 0,
        may_observe: 0,
    });
    let mut normal_exits = 0;
    for (index, block) in plan.blocks.iter().enumerate() {
        let fail = |message| error(Some(index), message);
        let mut flow = incoming[index].ok_or_else(|| fail("unreachable blocks are forbidden"))?;
        for (position, &op) in block.operations.iter().enumerate() {
            match op {
                Operation::SaveEntry => {
                    if index != 0 || position != 0 || flow.phase != Phase::Outside {
                        return Err(fail("SaveEntry must occur exactly once at the sole entry"));
                    }
                    flow.phase = Phase::Owned;
                }
                Operation::ReapplyEntryImage => {
                    if flow.phase != Phase::Owned {
                        return Err(fail(
                            "entry image can only be reapplied by its active owner",
                        ));
                    }
                    flow.phase = Phase::ImageReapplied;
                }
                Operation::Read { observation } | Operation::Swap { observation, .. } => {
                    if flow.phase != Phase::Owned {
                        return Err(fail(
                            "raw observation/swap requires the active ownership token",
                        ));
                    }
                    if let Operation::Swap { capture, .. } = op
                        && capture >= plan.captures.len()
                    {
                        return Err(fail("swap refers to an uncaptured input"));
                    }
                    if observation >= plan.observations {
                        return Err(fail("observation index out of range"));
                    }
                    let bit = 1u16 << observation;
                    if flow.may_observe & bit != 0 {
                        return Err(fail("observation may be overwritten on a normal path"));
                    }
                    flow.must_observe |= bit;
                    flow.may_observe |= bit;
                }
            }
        }
        let mut forward = |target: usize| -> Result<(), PlanError> {
            if target <= index || target >= plan.blocks.len() {
                return Err(fail("edges must target a later in-range block"));
            }
            match &mut incoming[target] {
                None => incoming[target] = Some(flow),
                Some(other) => {
                    if other.phase != flow.phase {
                        return Err(fail("join mixes ownership phases"));
                    }
                    other.must_observe &= flow.must_observe;
                    other.may_observe |= flow.may_observe;
                }
            }
            Ok(())
        };
        match block.exit {
            Exit::Jump(target) => forward(target)?,
            Exit::FlagsBranch {
                taken, not_taken, ..
            } => {
                if flow.phase != Phase::Owned {
                    return Err(fail("Jcc requires active owned FLAGS"));
                }
                forward(taken)?;
                forward(not_taken)?;
            }
            Exit::Normal => {
                if flow.phase != Phase::ImageReapplied {
                    return Err(fail("normal exit requires entry-image reapplication"));
                }
                if flow.must_observe != required {
                    return Err(fail("normal exit has undefined observations"));
                }
                normal_exits += 1;
            }
        }
    }
    Ok(StructuralReport {
        blocks: plan.blocks.len(),
        operations,
        normal_exits,
        unresolved: [
            Obligation::LegalEntryAndNormalExit,
            Obligation::CaptureAllocationAndPublication,
            Obligation::ExactMachineExpansion,
            Obligation::TerminalExceptionBoundary,
            Obligation::AsynchronousEnvironment,
        ],
    })
}

impl Condition {
    pub const ALL: [Self; 16] = [
        Self::Overflow,
        Self::NotOverflow,
        Self::Below,
        Self::AboveOrEqual,
        Self::Equal,
        Self::NotEqual,
        Self::BelowOrEqual,
        Self::Above,
        Self::Sign,
        Self::NotSign,
        Self::Parity,
        Self::NotParity,
        Self::Less,
        Self::GreaterOrEqual,
        Self::LessOrEqual,
        Self::Greater,
    ];
}
impl Plan {
    /// Closed raw CFG syntax used by the explicit terminal-monitor IR wrapper.
    pub fn parse(text: &str, captures: usize, observations: usize) -> Result<Self, String> {
        if text.len() > 32768 || captures > MAX_CAPTURES || observations > MAX_OBSERVATIONS {
            return Err("FLAGS region text/capture/result bound exceeded".into());
        }
        let mut blocks = Vec::new();
        for body in text.split('|') {
            if blocks.len() == MAX_BLOCKS {
                return Err("FLAGS region block bound exceeded".into());
            }
            let mut operations = Vec::new();
            let mut exit = None;
            for item in body.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                if exit.is_some() {
                    return Err("FLAGS region operation follows terminator".into());
                }
                let words = item.split_whitespace().collect::<Vec<_>>();
                let number = |i: usize| {
                    words[i]
                        .parse::<usize>()
                        .map_err(|_| "FLAGS region expected unsigned index".to_string())
                };
                match words.as_slice() {
                    ["save"] => operations.push(Operation::SaveEntry),
                    ["read", _] => operations.push(Operation::Read {
                        observation: number(1)?,
                    }),
                    ["swap", _, _] => operations.push(Operation::Swap {
                        capture: number(1)?,
                        observation: number(2)?,
                    }),
                    ["reapply"] => operations.push(Operation::ReapplyEntryImage),
                    ["jump", _] => exit = Some(Exit::Jump(number(1)?)),
                    ["jcc", _, _, _] => {
                        exit = Some(Exit::FlagsBranch {
                            condition: *Condition::ALL
                                .get(number(1)?)
                                .ok_or("FLAGS region Jcc condition must be 0..15")?,
                            taken: number(2)?,
                            not_taken: number(3)?,
                        })
                    }
                    ["normal"] => exit = Some(Exit::Normal),
                    _ => return Err(format!("unsupported FLAGS region instruction: {item}")),
                }
                if operations.len() > MAX_OPERATIONS {
                    return Err("FLAGS region operation bound exceeded".into());
                }
            }
            blocks.push(Block {
                operations,
                exit: exit.ok_or("FLAGS region block needs a terminator")?,
            });
        }
        let plan = Self {
            captures: vec![Type::I64; captures],
            observations,
            blocks,
        };
        analyze(&plan).map_err(|e| e.to_string())?;
        Ok(plan)
    }
}
impl std::fmt::Display for Plan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, block) in self.blocks.iter().enumerate() {
            if index != 0 {
                f.write_str(" | ")?;
            }
            for op in &block.operations {
                match op {
                    Operation::SaveEntry => f.write_str("save; ")?,
                    Operation::Read { observation } => write!(f, "read {observation}; ")?,
                    Operation::Swap {
                        capture,
                        observation,
                    } => write!(f, "swap {capture} {observation}; ")?,
                    Operation::ReapplyEntryImage => f.write_str("reapply; ")?,
                }
            }
            match block.exit {
                Exit::Jump(target) => write!(f, "jump {target}")?,
                Exit::FlagsBranch {
                    condition,
                    taken,
                    not_taken,
                } => write!(f, "jcc {} {taken} {not_taken}", condition as u8)?,
                Exit::Normal => f.write_str("normal")?,
            }
        }
        Ok(())
    }
}
