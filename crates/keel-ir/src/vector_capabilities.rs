//! Immutable optimizer capabilities, independent of a backend or CPU level name.
/// Accepted value width, complete recipe width, and preferred width are separate.
/// The feature fingerprint records the effective explicit x86 contract, not the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VectorCapabilities {
    value_bits: u16,
    signed_i32_sum_bits: u16,
    preferred_bits: u16,
    effective_x86_features: u64,
}
impl Default for VectorCapabilities {
    fn default() -> Self {
        Self::BASELINE
    }
}
impl VectorCapabilities {
    pub const BASELINE: Self = Self::new(false, false, false, 0);
    /// `integer256` describes the entire load/widen/add/reduce recipe.
    /// Preference never grants legality and is clamped to the accepted value width.
    /// Feature bits use the projection's documented order.
    pub const fn new(
        value256: bool,
        integer256: bool,
        prefer256: bool,
        effective_x86_features: u64,
    ) -> Self {
        Self {
            value_bits: if value256 { 256 } else { 128 },
            signed_i32_sum_bits: if value256 && integer256 { 256 } else { 128 },
            preferred_bits: if value256 && prefer256 { 256 } else { 128 },
            effective_x86_features,
        }
    }
    /// Explicit library profitability experiment; never adds accepted operations.
    /// An unsupported preference is conservatively normalized to 128 bits.
    pub const fn with_wide_preference(mut self, prefer256: bool) -> Self {
        self.preferred_bits = if prefer256 && self.value_bits == 256 {
            256
        } else {
            128
        };
        self
    }
    pub const fn value_bits(self) -> u16 {
        self.value_bits
    }
    pub const fn signed_i32_sum_bits(self) -> u16 {
        self.signed_i32_sum_bits
    }
    pub const fn preferred_bits(self) -> u16 {
        self.preferred_bits
    }
    pub const fn effective_x86_features(self) -> u64 {
        self.effective_x86_features
    }
    pub const fn prefer_wide_signed_i32_sum(self) -> bool {
        self.signed_i32_sum_bits == 256 && self.preferred_bits == 256
    }
}
