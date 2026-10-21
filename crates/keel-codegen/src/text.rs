//! Assembly text without nested `fmt::write` calls. Every helper prints
//! exactly what the corresponding `Display` would.
use std::fmt::{self, Write};

/// `prefix` followed by the decimal text of `n` in one `write_str`.
pub(crate) fn write_decimal<W: Write + ?Sized>(out: &mut W, prefix: &str, n: i128) -> fmt::Result {
    let mut buf = [0u8; 64];
    let mut i = buf.len();
    let mut magnitude = n.unsigned_abs();
    if let Ok(mut small) = u64::try_from(magnitude) {
        loop {
            i -= 1;
            buf[i] = b'0' + (small % 10) as u8;
            small /= 10;
            if small == 0 {
                break;
            }
        }
    } else {
        while magnitude != 0 {
            i -= 1;
            buf[i] = b'0' + (magnitude % 10) as u8;
            magnitude /= 10;
        }
    }
    if n < 0 {
        i -= 1;
        buf[i] = b'-';
    }
    i -= prefix.len();
    buf[i..i + prefix.len()].copy_from_slice(prefix.as_bytes());
    out.write_str(std::str::from_utf8(&buf[i..]).expect("ASCII digits"))
}

/// A one-character prefix such as a register class, then `n`.
pub(crate) fn write_class<W: Write + ?Sized>(out: &mut W, class: char, n: i128) -> fmt::Result {
    write_decimal(out, class.encode_utf8(&mut [0; 4]), n)
}

/// `{prefix}{function}_{block}` without `format!`.
pub(crate) fn block_label(prefix: &str, function: &str, block: keel_ir::Block) -> String {
    use keel_ir::EntityRef;
    let mut label = String::with_capacity(prefix.len() + function.len() + 16);
    label.push_str(prefix);
    label.push_str(function);
    write_decimal(&mut label, "_block", block.index() as i128).unwrap();
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_display() {
        for n in [
            0,
            1,
            -1,
            9,
            10,
            -10,
            4095,
            i64::MIN as i128,
            u64::MAX as i128,
        ]
        .into_iter()
        .chain([
            i128::MIN,
            i128::MAX,
            u64::MAX as i128 + 1,
            -(u64::MAX as i128) - 1,
        ]) {
            let mut s = String::new();
            write_decimal(&mut s, "#", n).unwrap();
            assert_eq!(s, format!("#{n}"));
            s.clear();
            write_class(&mut s, 'x', n).unwrap();
            assert_eq!(s, format!("x{n}"));
        }
    }
}
