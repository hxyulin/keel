//! Exact-width MOVZ/MOVN seed followed by MOVK patches. No logical-immediate
//! seeds or implicit W definitions of requested X values are considered.
//!
//! A seed supplies one arbitrary halfword and fills all other lanes with zeros
//! (MOVZ) or ones (MOVN). Every remaining differing lane needs one MOVK, so
//! enumerating both fills at every legal lane is minimal within this family.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Plan {
    bits: u64,
    lanes: u8,
    seed_lane: u8,
    inverted: bool,
}

impl Plan {
    pub(super) fn new(bits: u64, is64: bool) -> Self {
        let mut best = Self {
            bits: if is64 { bits } else { bits & 0xffff_ffff },
            lanes: if is64 { 4 } else { 2 },
            seed_lane: 0,
            inverted: false,
        };
        let mut best_cost = usize::MAX;
        // Strict improvement makes ties deterministic: MOVZ before MOVN,
        // then the lowest seed lane. Patches are emitted in ascending order.
        for inverted in [false, true] {
            for seed_lane in 0..best.lanes {
                let candidate = Self {
                    seed_lane,
                    inverted,
                    ..best
                };
                let cost = 1 + candidate.patches().count();
                if cost < best_cost {
                    best = candidate;
                    best_cost = cost;
                }
            }
        }
        best
    }

    pub(super) fn inverted(self) -> bool {
        self.inverted
    }

    pub(super) fn seed(self) -> (u8, u16) {
        // MOVN complements the whole requested word. Complementing this
        // halfword encodes the target lane and leaves every other lane FFFF.
        let half = self.half(self.seed_lane);
        (self.seed_lane, if self.inverted { !half } else { half })
    }

    pub(super) fn patches(self) -> impl Iterator<Item = (u8, u16)> {
        let fill = if self.inverted { u16::MAX } else { 0 };
        (0..self.lanes)
            .filter(move |&lane| lane != self.seed_lane && self.half(lane) != fill)
            .map(move |lane| (lane, self.half(lane)))
    }

    fn half(self, lane: u8) -> u16 {
        (self.bits >> (16 * lane)) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::Plan;

    fn check(bits: u64, is64: bool) {
        let mask = if is64 { u64::MAX } else { u32::MAX as u64 };
        let expected = bits & mask;
        let plan = Plan::new(bits, is64);
        let (lane, imm) = plan.seed();
        assert!(lane < if is64 { 4 } else { 2 });
        let mut value = u64::from(imm) << (16 * lane);
        if plan.inverted() {
            value = !value;
        }
        value &= mask;
        let mut previous = None;
        for (patch_lane, patch) in plan.patches() {
            assert_ne!(patch_lane, lane);
            assert!(patch_lane < if is64 { 4 } else { 2 });
            assert!(previous.is_none_or(|p| p < patch_lane));
            previous = Some(patch_lane);
            let shift = 16 * patch_lane;
            value = (value & !(0xffff_u64 << shift)) | (u64::from(patch) << shift);
        }
        assert_eq!(value, expected, "{bits:#018x}, is64={is64}, {plan:?}");

        // Independent lower bound: a zero/one seed can supply at most one
        // nonzero/non-FFFF target halfword; every other such half needs MOVK.
        let bytes = expected.to_le_bytes();
        let halves = bytes[..if is64 { 8 } else { 4 }].as_chunks::<2>().0.iter();
        let nonzero = halves.clone().filter(|h| **h != [0, 0]).count();
        let nonones = halves.filter(|h| **h != [255, 255]).count();
        let minimum = nonzero.min(nonones).max(1);
        assert_eq!(1 + plan.patches().count(), minimum, "{plan:?}");
    }

    #[test]
    fn all_lane_classes_and_deterministic_random_values_are_exact_and_minimal() {
        for is64 in [false, true] {
            // Bounded exhaustive combinations of zero, all-ones, and two
            // distinct nontrivial halves exercise both fills and all lanes.
            for mut code in 0..256 {
                let mut bits = 0;
                for lane in 0..4 {
                    let half = [0_u64, 0xffff, 0x1234, 0xabcd][code % 4];
                    code /= 4;
                    bits |= half << (16 * lane);
                }
                check(bits, is64);
            }
            let mut bits = 0x1234_5678_9abc_def0_u64;
            for _ in 0..512 {
                bits ^= bits << 13;
                bits ^= bits >> 7;
                bits ^= bits << 17;
                check(bits, is64);
                check(!bits, is64);
            }
            for bits in [0, u64::MAX, 1 << 63, 0x7ff8_0000_0000_0123, 0xffc0_0042] {
                check(bits, is64);
            }
        }
    }

    #[test]
    fn ties_choose_movz_then_lowest_seed_lane() {
        for bits in [0, 0x1234_5678, 0xffff_0000_ffff_0000] {
            let plan = Plan::new(bits, true);
            assert!(!plan.inverted());
            let first_nonzero = (0..4).find(|lane| (bits >> (16 * lane)) as u16 != 0);
            assert_eq!(plan.seed().0, first_nonzero.unwrap_or(0));
        }
        let ones = Plan::new(u64::MAX, true);
        assert!(ones.inverted());
        assert_eq!(ones.seed(), (0, 0));
        let high = Plan::new(0xabcd << 48, true);
        assert_eq!(high.seed(), (3, 0xabcd));
        assert_eq!(high.patches().count(), 0);
        let inverted = Plan::new(0xffff_abcd_ffff_ffff, true);
        assert!(inverted.inverted());
        assert_eq!(inverted.seed(), (2, !0xabcd));
        assert_eq!(inverted.patches().count(), 0);
    }

    #[test]
    fn w_requests_clear_upper_bits_but_x_requests_never_narrow() {
        let w = Plan::new(0xabcd_ffff_ffff_ffff, false);
        assert_eq!(w.seed(), (0, 0));
        assert!(w.inverted());
        assert_eq!(w.patches().count(), 0);
        // A W MOVN #0 would produce the wrong X result for this request.
        let x = Plan::new(0x0000_0000_ffff_ffff, true);
        assert!(!x.inverted());
        assert_eq!(x.patches().count(), 1);
        check(0xabcd_ffff_ffff_ffff, false);
        check(0x0000_0000_ffff_ffff, true);
    }
}
