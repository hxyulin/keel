//! CPU profitability tuning, separate from ISA legality and optimization effort.

use crate::{InstructionSelector, OptionsSelector, SelectionOptions, Target};
use keel_ir::{Function, InstData, Value};

mod apple_m5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TuningCpu {
    #[default]
    Generic,
    AppleM5,
}
impl TuningCpu {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "generic" => Some(Self::Generic),
            "apple-m5" => Some(Self::AppleM5),
            _ => None,
        }
    }

    pub fn validate_target(self, target: Target) -> Result<(), &'static str> {
        crate::require_native_target(target)?;
        if self == Self::AppleM5 && target != Target::Aarch64AppleDarwin {
            return Err("apple-m5 tuning requires aarch64-apple-darwin");
        }
        Ok(())
    }

    /// Return only alternatives supported by this CPU's calibration.
    pub(crate) fn multiply_shift(self, bits: u32, constant: i64) -> Option<u8> {
        if self != Self::AppleM5 {
            return None;
        }
        apple_m5::MULTIPLY_ALTERNATIVES
            .iter()
            .find(|cost| {
                cost.bits == bits
                    && cost.constant == constant
                    && cost.chain_ratio_upper < 1.0
                    && cost.parallel_ratio <= 1.0
            })
            .map(|cost| cost.shift)
    }
}

struct Alternative {
    bits: u32,
    constant: i64,
    shift: u8,
    chain_ratio_upper: f64,
    parallel_ratio: f64,
}

pub struct TunedSelector {
    pub options: SelectionOptions,
    pub cpu: TuningCpu,
}
impl InstructionSelector for TunedSelector {
    fn name(&self) -> &'static str {
        match self.cpu {
            TuningCpu::Generic => "generic",
            TuningCpu::AppleM5 => "apple-m5",
        }
    }

    fn validate_target(&self, target: Target) -> Result<(), &'static str> {
        self.cpu.validate_target(target)
    }

    fn select(
        &self,
        func: &Function,
        target: crate::select::Target,
        folds: &dyn Fn(&InstData) -> Vec<Value>,
    ) -> crate::select::Selection {
        self.select_counted(func, target, folds, None)
    }

    fn select_with_uses(
        &self,
        func: &Function,
        target: crate::select::Target,
        folds: &dyn Fn(&InstData) -> Vec<Value>,
        uses: &[u8],
    ) -> crate::select::Selection {
        self.select_counted(func, target, folds, Some(uses))
    }
}

impl TunedSelector {
    fn select_counted(
        &self,
        func: &Function,
        target: crate::select::Target,
        folds: &dyn Fn(&InstData) -> Vec<Value>,
        uses: Option<&[u8]>,
    ) -> crate::select::Selection {
        if self.cpu == TuningCpu::Generic {
            let options = OptionsSelector(self.options);
            return match uses {
                Some(uses) => options.select_with_uses(func, target, folds, uses),
                None => options.select(func, target, folds),
            };
        }
        profiling::scope!("instruction-selection", "apple-m5");
        let mut selection =
            crate::select::Selection::compute_with_uses(func, target, self.options, uses);
        if self.options.trees && target == crate::select::Target::Aarch64 {
            selection.tune_multiplications(func, self.cpu);
        }
        if self.options.folded_operands {
            selection.fold_operands(func, folds);
        }
        selection
    }
}
