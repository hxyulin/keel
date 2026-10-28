//! Driver presets combine independent IR, selection and allocation policies.
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use keel_opt::OptimizationTier;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    O0,
    O1,
    O2,
    O3,
    Size,
}

impl Level {
    pub(crate) fn parse(flag: &str) -> Option<Self> {
        Some(match flag {
            "-O0" => Self::O0,
            "-O1" => Self::O1,
            "-O2" => Self::O2,
            "-O" | "-O3" => Self::O3,
            "-Os" => Self::Size,
            _ => return None,
        })
    }

    pub(crate) fn optimization(self) -> OptimizationTier {
        match self {
            Self::O0 => OptimizationTier::None,
            Self::O1 => OptimizationTier::Fast,
            Self::O2 => OptimizationTier::Balanced,
            Self::O3 => OptimizationTier::Full,
            Self::Size => OptimizationTier::Size,
        }
    }

    pub(crate) fn selection(self) -> SelectionTier {
        match self {
            Self::O0 => SelectionTier::Baseline,
            Self::O1 => SelectionTier::Trees,
            Self::O2 | Self::O3 | Self::Size => SelectionTier::Full,
        }
    }

    pub(crate) fn allocation(self) -> AllocationTier {
        match self {
            Self::O1 => AllocationTier::Fast,
            Self::O0 | Self::O2 | Self::O3 | Self::Size => AllocationTier::Full,
        }
    }
}
