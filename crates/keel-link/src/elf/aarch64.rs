//! AArch64 ELF relocation fields. All arithmetic is checked before insertion.
use crate::Result;

pub(crate) fn width(kind: u32) -> Result<u64> {
    match kind {
        512..=573 => Ok(4),
        257 | 260 => Ok(8),
        258 | 261 | 273..=280 | 282..=286 | 299 | 311 | 312 => Ok(4),
        _ => Err(format!("unsupported AArch64 relocation {kind}").into()),
    }
}
pub(super) fn got(kind: u32) -> bool {
    matches!(kind, 311 | 312)
}

pub(super) fn apply(kind: u32, value: i128, place: i128, field: &[u8]) -> Result<Vec<u8>> {
    // TLS GOT-address fields use the ordinary instruction encodings.
    let alias = match kind {
        512 | 517 | 561 => Some(274),
        513 | 518 | 541 | 562 => Some(275),
        514 | 519 | 564 => Some(277),
        522 | 543 | 560 => Some(273),
        542 | 563 => Some(312),
        528..=530 => Some(kind + 21),
        _ => None,
    };
    if let Some(alias) = alias {
        return apply(alias, value, place, field);
    }
    let overflow = || {
        crate::Error::from(format!(
            "AArch64 relocation {kind} overflow or misalignment"
        ))
    };
    let signed = |n: i128, bits: u32, scale: u32| -> Result<u32> {
        if n % (1i128 << scale) != 0
            || n < -(1i128 << (bits + scale - 1))
            || n >= 1i128 << (bits + scale - 1)
        {
            return Err(overflow());
        }
        Ok(((n >> scale) & ((1i128 << bits) - 1)) as u32)
    };
    match kind {
        257 => {
            return u64::try_from(value)
                .map(|v| v.to_le_bytes().to_vec())
                .map_err(|_| overflow());
        }
        258 => {
            return u32::try_from(value)
                .map(|v| v.to_le_bytes().to_vec())
                .map_err(|_| overflow());
        }
        260 => {
            return i64::try_from(value - place)
                .map(|v| v.to_le_bytes().to_vec())
                .map_err(|_| overflow());
        }
        261 => {
            return i32::try_from(value - place)
                .map(|v| v.to_le_bytes().to_vec())
                .map_err(|_| overflow());
        }
        _ => {}
    }
    let mut word = u32::from_le_bytes(field[..4].try_into().unwrap());
    let instruction = |valid: bool| -> Result<()> {
        if valid {
            Ok(())
        } else {
            Err(format!("AArch64 relocation {kind} does not match instruction").into())
        }
    };
    match kind {
        515 | 516 | 520 | 521 | 523..=527 | 539 | 540 | 544..=548 | 565 | 566 => {
            let (group, checked) = match kind {
                515 | 520 | 539 | 565 | 524 | 545 => (1, true),
                523 | 544 => (2, true),
                525 | 546 => (1, false),
                526 | 547 => (0, true),
                _ => (0, false),
            };
            instruction(word & 0x1f800000 == 0x12800000 && word >> 29 & 3 != 1)?;
            if checked
                && (value < -(1i128 << (16 * (group + 1))) || value >= 1i128 << (16 * (group + 1)))
            {
                return Err(overflow());
            }
            let immediate = (((if checked && value < 0 { !value } else { value }) >> (16 * group))
                & 65535) as u32;
            if checked {
                word = (word & !(3 << 29)) | if value < 0 { 0 } else { 2 << 29 }; // MOVN or MOVZ
            }
            word = (word & !((65535 << 5) | (3 << 21))) | immediate << 5 | group << 21;
        }
        531..=538 | 552..=559 | 570..=573 => {
            let (scale, checked) = match kind {
                531..=538 => ((kind - 531) / 2, !kind.is_multiple_of(2)),
                552..=559 => ((kind - 552) / 2, kind.is_multiple_of(2)),
                570..=573 => (4, kind.is_multiple_of(2)),
                _ => unreachable!(),
            };
            if checked && !(0..4096).contains(&value) {
                return Err(overflow());
            }
            return apply(
                match scale {
                    0 => 278,
                    1 => 284,
                    2 => 285,
                    3 => 286,
                    _ => 299,
                },
                value,
                place,
                field,
            );
        }
        567 => {
            instruction(word & 0xffc00000 == 0xf9400000)?;
        }
        568 => {
            instruction(word & 0xffc00000 == 0x91000000)?;
        }
        569 => {
            instruction(word & 0xfffffc1f == 0xd63f0000)?;
        }
        549..=551 => {
            instruction(word & 0x7f000000 == 0x11000000)?;
            if value < 0 || (kind != 551 && value >= 1 << 24) || (kind == 550 && value >= 4096) {
                return Err(overflow());
            }
            let immediate = if kind == 549 {
                (value >> 12) & 4095
            } else {
                value & 4095
            };
            word = (word & !((4095 << 10) | (1 << 22)))
                | (immediate as u32) << 10
                | if kind == 549 { 1 << 22 } else { 0 };
        }
        282 | 283 => {
            instruction(word & 0xfc000000 == if kind == 283 { 0x94000000 } else { 0x14000000 })?;
            word = (word & !0x03ffffff) | signed(value - place, 26, 2)?;
        }
        273 | 280 => {
            instruction(if kind == 273 {
                word & 0x3b000000 == 0x18000000
            } else {
                word & 0xff000010 == 0x54000000 || word & 0x7e000000 == 0x34000000
            })?;
            word = (word & !(0x7ffff << 5)) | signed(value - place, 19, 2)? << 5;
        }
        279 => {
            instruction(word & 0x7e000000 == 0x36000000)?;
            word = (word & !(0x3fff << 5)) | signed(value - place, 14, 2)? << 5;
        }
        274 | 275 | 276 | 311 => {
            let page = kind != 274;
            instruction(word & 0x9f000000 == if page { 0x90000000 } else { 0x10000000 })?;
            let delta = if page {
                (value & !4095) - (place & !4095)
            } else {
                value - place
            };
            let imm = if kind == 276 {
                ((delta >> 12) & 0x1fffff) as u32
            } else {
                signed(delta, 21, if page { 12 } else { 0 })?
            };
            word = (word & !((3 << 29) | (0x7ffff << 5))) | (imm & 3) << 29 | (imm >> 2) << 5;
        }
        277 => {
            instruction(word & 0x7fc00000 == 0x11000000)?;
            word = (word & !(4095 << 10)) | ((value & 4095) as u32) << 10;
        }
        278 | 284..=286 | 299 | 312 => {
            let scale = match kind {
                278 => 0,
                284 => 1,
                285 => 2,
                299 => 4,
                _ => 3,
            };
            instruction(word & 0x3b000000 == 0x39000000)?;
            let actual = if word & 0x04800000 == 0x04800000 {
                4
            } else {
                word >> 30
            };
            instruction(actual == scale && (kind != 312 || word & 0xffc00000 == 0xf9400000))?;
            let low = value & 4095;
            if low % (1 << scale) != 0 {
                return Err(overflow());
            }
            word = (word & !(4095 << 10)) | ((low >> scale) as u32) << 10;
        }
        _ => return Err(format!("unsupported AArch64 relocation {kind}").into()),
    }
    Ok(word.to_le_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::apply;

    #[test]
    fn tls_local_exec_add_fields_and_overflow() {
        let add = 0x91000000u32.to_le_bytes();
        let hi = apply(549, 0x123456, 0, &add).unwrap();
        let lo = apply(551, 0x123456, 0, &add).unwrap();
        assert_eq!(u32::from_le_bytes(hi.try_into().unwrap()), 0x91448c00);
        assert_eq!(u32::from_le_bytes(lo.try_into().unwrap()), 0x91115800);
        assert!(apply(549, 1 << 24, 0, &add).is_err());
        assert!(apply(550, 4096, 0, &add).is_err());
        assert!(apply(550, 4095, 0, &add).is_ok());
        assert!(apply(549, 16, 0, &[0; 4]).is_err());
    }
}
