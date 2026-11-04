//! ELF relocation fields shared by the default and script executable paths.

/// x86-64 relocation fields. The width table and the arithmetic live together
/// so a kind cannot be accepted at parse time without an encoding.
pub(super) mod x86_64 {
    use crate::Result;
    use object::elf::{
        R_X86_64_32, R_X86_64_32S, R_X86_64_64, R_X86_64_GOTPCREL, R_X86_64_GOTPCRELX,
        R_X86_64_PC8, R_X86_64_PC32, R_X86_64_PC64, R_X86_64_PLT32, R_X86_64_REX_GOTPCRELX,
    };

    /// Field width in bytes of a supported relocation.
    pub(in crate::elf) fn width(kind: u32) -> Result<u64> {
        match kind {
            // R_X86_64_PC8 is used by LOOP and J*CXZ.
            R_X86_64_PC8 => Ok(1),
            R_X86_64_64 | R_X86_64_PC64 => Ok(8),
            R_X86_64_PC32
            | R_X86_64_PLT32
            | R_X86_64_GOTPCREL
            | R_X86_64_32
            | R_X86_64_32S
            | R_X86_64_GOTPCRELX
            | R_X86_64_REX_GOTPCRELX => Ok(4),
            _ => Err(format!("unsupported x86-64 relocation {kind}")),
        }
    }

    /// Whether the field addresses the symbol's GOT slot instead of the symbol.
    pub(in crate::elf) fn uses_got(kind: u32) -> bool {
        matches!(
            kind,
            R_X86_64_GOTPCREL | R_X86_64_GOTPCRELX | R_X86_64_REX_GOTPCRELX
        )
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(in crate::elf) enum Error {
        Unsupported,
        Overflow,
    }

    /// An encoded little-endian field and its width in bytes.
    #[derive(Debug, PartialEq, Eq)]
    pub(in crate::elf) struct Field {
        pub bytes: [u8; 8],
        pub width: usize,
    }

    /// Encode `value` (S + A, or G + A for GOT kinds) at `place` (P).
    ///
    /// ELF64 address arithmetic wraps modulo 2^64 before a field is narrowed,
    /// so `value` is reduced to 64 bits and a PC-relative field holds the
    /// wrapped difference read as signed. A reference across the top of the
    /// address space, or across the canonical-address gap with R_X86_64_PC64,
    /// encodes the short distance instead of overflowing.
    pub(in crate::elf) fn encode(
        kind: u32,
        value: i128,
        place: u64,
    ) -> std::result::Result<Field, Error> {
        let value = value as u64;
        let pc = value.wrapping_sub(place) as i64;
        let (n, width) = match kind {
            R_X86_64_64 => (value, 8),
            R_X86_64_32 => (
                u64::from(u32::try_from(value).map_err(|_| Error::Overflow)?),
                4,
            ),
            R_X86_64_32S => (
                i32::try_from(value as i64).map_err(|_| Error::Overflow)? as u64,
                4,
            ),
            R_X86_64_PC8 => (i8::try_from(pc).map_err(|_| Error::Overflow)? as u64, 1),
            R_X86_64_PC64 => (pc as u64, 8),
            R_X86_64_PC32
            | R_X86_64_PLT32
            | R_X86_64_GOTPCREL
            | R_X86_64_GOTPCRELX
            | R_X86_64_REX_GOTPCRELX => (i32::try_from(pc).map_err(|_| Error::Overflow)? as u64, 4),
            _ => return Err(Error::Unsupported),
        };
        Ok(Field {
            bytes: n.to_le_bytes(),
            width,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn field(kind: u32, value: i128, place: u64) -> std::result::Result<Vec<u8>, Error> {
            encode(kind, value, place).map(|f| f.bytes[..f.width].to_vec())
        }

        #[test]
        fn every_width_has_an_encoding() {
            for kind in 0..64 {
                match (width(kind), encode(kind, 0, 0)) {
                    (Ok(w), Ok(f)) => assert_eq!(w as usize, f.width, "kind {kind}"),
                    (Err(_), Err(Error::Unsupported)) => {}
                    other => panic!("kind {kind}: {other:?}"),
                }
            }
        }

        #[test]
        fn pc32_boundaries_use_the_wrapped_distance() {
            let place = 0x40_1000u64;
            let at = |d: i128| i128::from(place) + d;
            for kind in [R_X86_64_PC32, R_X86_64_PLT32, R_X86_64_GOTPCRELX] {
                assert_eq!(
                    field(kind, at(0x7fff_ffff), place),
                    Ok(0x7fff_ffffu32.to_le_bytes().to_vec())
                );
                assert_eq!(
                    field(kind, at(-0x8000_0000), place),
                    Ok(0x8000_0000u32.to_le_bytes().to_vec())
                );
                assert_eq!(field(kind, at(0x8000_0000), place), Err(Error::Overflow));
                assert_eq!(field(kind, at(-0x8000_0001), place), Err(Error::Overflow));
            }
            // A high-half place reaching low addresses wraps through 2^64.
            let high = 0xffff_ffff_8000_0010u64;
            assert_eq!(
                field(R_X86_64_PC32, 0, high),
                Ok(0x7fff_fff0u32.to_le_bytes().to_vec())
            );
            assert_eq!(
                field(R_X86_64_PC32, 0x10, 0xffff_ffff_ffff_fff0),
                Ok(0x20u32.to_le_bytes().to_vec())
            );
            // A negative S + A is the high-half address it wraps to.
            assert_eq!(
                field(R_X86_64_PC32, -0x10, 0xffff_ffff_ffff_fff0),
                Ok(0u32.to_le_bytes().to_vec())
            );
            assert_eq!(field(R_X86_64_PC32, 0x1000, high), Err(Error::Overflow));
        }

        #[test]
        fn pc64_crosses_the_canonical_address_gap() {
            let low_top = 0x0000_7fff_ffff_fff0u64;
            let high_bottom = 0xffff_8000_0000_0000u64;
            let up = high_bottom.wrapping_sub(low_top);
            assert_eq!(
                field(R_X86_64_PC64, high_bottom.into(), low_top),
                Ok(up.to_le_bytes().to_vec())
            );
            assert_eq!(
                field(R_X86_64_PC64, low_top.into(), high_bottom),
                Ok(up.wrapping_neg().to_le_bytes().to_vec())
            );
            // No 32-bit field spans the gap.
            assert_eq!(
                field(R_X86_64_PC32, high_bottom.into(), low_top),
                Err(Error::Overflow)
            );
        }

        #[test]
        fn pc8_boundaries() {
            assert_eq!(field(R_X86_64_PC8, 0x107f, 0x1000), Ok(vec![0x7f]));
            assert_eq!(field(R_X86_64_PC8, 0x0f80, 0x1000), Ok(vec![0x80]));
            assert_eq!(field(R_X86_64_PC8, 0x1080, 0x1000), Err(Error::Overflow));
            assert_eq!(field(R_X86_64_PC8, 0x0f7f, 0x1000), Err(Error::Overflow));
            assert_eq!(field(R_X86_64_PC8, 0, u64::MAX), Ok(vec![1]));
        }

        #[test]
        fn absolute_fields_narrow_the_wrapped_value() {
            assert_eq!(
                field(R_X86_64_64, -1, 0),
                Ok(u64::MAX.to_le_bytes().to_vec())
            );
            assert_eq!(field(R_X86_64_64, 1i128 << 64, 0), Ok(vec![0; 8]));
            assert_eq!(field(R_X86_64_32, 0xffff_ffff, 0), Ok(vec![0xff; 4]));
            assert_eq!(field(R_X86_64_32, 0x1_0000_0000, 0), Err(Error::Overflow));
            assert_eq!(field(R_X86_64_32, -1, 0), Err(Error::Overflow));
            assert_eq!(
                field(R_X86_64_32S, 0x7fff_ffff, 0),
                Ok(vec![0xff, 0xff, 0xff, 0x7f])
            );
            assert_eq!(
                field(R_X86_64_32S, -0x8000_0000, 0),
                Ok(vec![0, 0, 0, 0x80])
            );
            assert_eq!(
                field(R_X86_64_32S, 0xffff_ffff_8000_0000, 0),
                Ok(vec![0, 0, 0, 0x80])
            );
            assert_eq!(field(R_X86_64_32S, 0x8000_0000, 0), Err(Error::Overflow));
        }
    }
}
