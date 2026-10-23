//! AArch64 ELF relocation fields. All arithmetic is checked before insertion.
use crate::Result;

pub(crate) fn width(kind: u32) -> Result<u64> {
    match kind {
        257 | 260 => Ok(8),
        258 | 261 | 273..=280 | 282..=286 | 299 | 311 | 312 => Ok(4),
        _ => Err(format!("unsupported AArch64 relocation {kind}")),
    }
}
pub(super) fn got(kind: u32) -> bool {
    matches!(kind, 311 | 312)
}

pub(super) fn apply(kind: u32, value: i128, place: i128, field: &[u8]) -> Result<Vec<u8>> {
    let overflow = || format!("AArch64 relocation {kind} overflow or misalignment");
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
            Err(format!(
                "AArch64 relocation {kind} does not match instruction"
            ))
        }
    };
    match kind {
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
        _ => return Err(format!("unsupported AArch64 relocation {kind}")),
    }
    Ok(word.to_le_bytes().to_vec())
}
