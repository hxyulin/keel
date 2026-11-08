//! Validate the fixed-width fields of four cache fixups at a time.
use super::FixupRecord;
use std::arch::aarch64::{
    uint32x4_t, vaddq_u32, vandq_u32, vbslq_u32, vceqq_u32, vcgeq_u32, vcgtq_u32, vcleq_u32,
    vcltq_u32, vdupq_n_u32, vextq_u32, vgetq_lane_u32, vld3q_u32, vminvq_u32, vorrq_u32,
    vshrq_n_u32, vsubq_u32, vuzp1q_u32, vuzp2q_u32,
};

#[target_feature(enable = "neon")]
fn valid_target(targets: uint32x4_t, atoms: uint32x4_t, symbols: uint32x4_t) -> uint32x4_t {
    let ids = vandq_u32(targets, vdupq_n_u32(0x7fff_ffff));
    let local = vcgtq_u32(targets, vdupq_n_u32(0x7fff_ffff));
    vcltq_u32(ids, vbslq_u32(local, atoms, symbols))
}

/// Return the validated prefix length and its final write end. A failed batch
/// returns an empty prefix so scalar validation preserves diagnostic order.
#[target_feature(enable = "neon")]
fn vector_prefix(
    records: &[FixupRecord],
    size: usize,
    atoms: usize,
    symbols: usize,
) -> (usize, usize) {
    if records.len() < 4 || size < 8 || size > (u32::MAX - 8) as usize {
        return (0, 0);
    }
    let atoms = vdupq_n_u32(atoms.min(u32::MAX as usize) as u32);
    let symbols = vdupq_n_u32(symbols.min(u32::MAX as usize) as u32);
    let mut end = 0;
    for batch in records.as_chunks::<4>().0 {
        let pointer = batch.as_ptr().cast::<u32>();
        // SAFETY: Four repr(C), alignment-one records occupy exactly 96 bytes.
        // Each unaligned NEON load reads 48 bytes within this batch. This module
        // only compiles for little-endian AArch64 with NEON enabled, matching
        // the cache's little-endian fields and the intrinsic's CPU requirement.
        let (first, second) = unsafe { (vld3q_u32(pointer), vld3q_u32(pointer.add(12))) };
        let offsets = vuzp1q_u32(first.0, second.0);
        let targets = vuzp1q_u32(first.1, second.1);
        let aux = vuzp2q_u32(first.1, second.1);
        let controls = vuzp2q_u32(first.2, second.2);
        let kinds = vandq_u32(controls, vdupq_n_u32(0xffff));
        let pairs = vcgeq_u32(kinds, vdupq_n_u32(11));
        let differences = vandq_u32(
            vcgeq_u32(kinds, vdupq_n_u32(9)),
            vcleq_u32(kinds, vdupq_n_u32(10)),
        );
        let wide = vorrq_u32(
            pairs,
            vorrq_u32(
                vceqq_u32(kinds, vdupq_n_u32(0)),
                vceqq_u32(kinds, vdupq_n_u32(10)),
            ),
        );
        let widths = vbslq_u32(wide, vdupq_n_u32(8), vdupq_n_u32(4));
        let ends = vaddq_u32(offsets, widths);
        let previous = vextq_u32::<3>(vdupq_n_u32(end), ends);
        let mut valid = vandq_u32(
            vceqq_u32(vshrq_n_u32::<16>(controls), vdupq_n_u32(0)),
            vcleq_u32(kinds, vdupq_n_u32(13)),
        );
        valid = vandq_u32(valid, valid_target(targets, atoms, symbols));
        valid = vandq_u32(valid, vcgeq_u32(offsets, previous));
        valid = vandq_u32(
            valid,
            vcleq_u32(offsets, vsubq_u32(vdupq_n_u32(size as u32), widths)),
        );
        let valid_aux = vbslq_u32(
            pairs,
            vceqq_u32(aux, vaddq_u32(offsets, vdupq_n_u32(4))),
            vbslq_u32(
                differences,
                valid_target(aux, atoms, symbols),
                vceqq_u32(aux, vdupq_n_u32(0)),
            ),
        );
        valid = vandq_u32(valid, valid_aux);
        if vminvq_u32(valid) != u32::MAX {
            return (0, 0);
        }
        end = vgetq_lane_u32::<3>(ends);
    }
    (records.len() / 4 * 4, end as usize)
}

pub(super) fn prefix(
    records: &[FixupRecord],
    size: usize,
    atoms: usize,
    symbols: usize,
) -> (usize, usize) {
    // SAFETY: This module only compiles when NEON is enabled on the target.
    unsafe { vector_prefix(records, size, atoms, symbols) }
}

#[cfg(test)]
mod tests {
    use super::super::{K, fixup};
    use super::*;
    use zerocopy::byteorder::little_endian::{I64, U16, U32};
    use zerocopy::{FromBytes, IntoBytes};

    fn scalar_valid(records: &[FixupRecord]) -> bool {
        let target_valid = |t| match t {
            crate::Target::Atom(n) | crate::Target::Symbol(n) => n < 3,
        };
        let mut end = 0;
        for record in records {
            let Ok(f) = fixup(record) else {
                return false;
            };
            if f.offset < end || !target_valid(f.target) {
                return false;
            }
            let width = if f.kind.low_offset().is_some() {
                8
            } else {
                f.kind.width() as u32
            };
            let Some(next) = f.offset.checked_add(width) else {
                return false;
            };
            if next > 128
                || f.kind
                    .low_offset()
                    .is_some_and(|low| f.offset.checked_add(4) != Some(low))
                || matches!(f.kind, K::Difference32 { minus } | K::Difference64 { minus } if !target_valid(minus))
            {
                return false;
            }
            end = next;
        }
        true
    }

    #[test]
    fn vector_checks_match_scalar_for_every_kind_lane_and_field_bit() {
        for first_kind in 0..14 {
            let records: Vec<_> = (0..8)
                .map(|i| {
                    let kind = (first_kind + i) % 14;
                    FixupRecord {
                        offset: U32::new(i * 16),
                        target: U32::new(if i % 2 == 0 { 1 } else { 0x8000_0002 }),
                        addend: I64::new(-71),
                        aux: U32::new(if kind >= 11 {
                            i * 16 + 4
                        } else if kind >= 9 {
                            0x8000_0001
                        } else {
                            0
                        }),
                        kind: U16::new(kind as u16),
                        reserved: U16::new(0),
                    }
                })
                .collect();
            assert_eq!(
                prefix(&records, 128, 3, 3),
                (
                    8,
                    if records[7].kind.get() == 0
                        || records[7].kind.get() == 10
                        || records[7].kind.get() >= 11
                    {
                        120
                    } else {
                        116
                    }
                )
            );
            assert_eq!(prefix(&records[..7], 128, 3, 3).0, 4);
            assert_eq!(prefix(&records[..3], 128, 3, 3), (0, 0));
            assert_eq!(prefix(&records, 7, 3, 3), (0, 0));
            assert_eq!(prefix(&records, usize::MAX, 3, 3), (0, 0));
            // Include an unaligned base, as supported by the public cache API.
            let mut bytes = vec![0xff];
            bytes.extend_from_slice(records.as_bytes());
            for byte in 1..bytes.len() {
                for bit in 0..8 {
                    bytes[byte] ^= 1 << bit;
                    let changed = <[FixupRecord]>::ref_from_bytes(&bytes[1..]).unwrap();
                    assert_eq!(
                        prefix(changed, 128, 3, 3).0 == 8,
                        scalar_valid(changed),
                        "kind {first_kind}, byte {byte}, bit {bit}"
                    );
                    bytes[byte] ^= 1 << bit;
                }
            }
        }
    }
}
