//! Assembly text without nested `fmt::write` calls. Every helper prints
//! exactly what the corresponding `Display` would.
use std::fmt::{self, Write};

/// The decimal text of 0 through 999, back to back.
const SMALL: &str = {
    const BYTES: [u8; 2890] = {
        let mut bytes = [0u8; 2890];
        let (mut n, mut i) = (0, 0);
        while n < 1000 {
            if n >= 100 {
                bytes[i] = b'0' + (n / 100) as u8;
                i += 1;
            }
            if n >= 10 {
                bytes[i] = b'0' + (n / 10 % 10) as u8;
                i += 1;
            }
            bytes[i] = b'0' + (n % 10) as u8;
            i += 1;
            n += 1;
        }
        bytes
    };
    match std::str::from_utf8(&BYTES) {
        Ok(text) => text,
        Err(_) => panic!("ASCII digits"),
    }
};

/// The decimal text of `n < 1000`, without validating a digit buffer.
fn small(n: usize) -> &'static str {
    let (start, len) = match n {
        0..10 => (n, 1),
        10..100 => (10 + 2 * (n - 10), 2),
        _ => (190 + 3 * (n - 100), 3),
    };
    &SMALL[start..start + len]
}

/// `000` through `999`, back to back.
const PADDED: &str = {
    const BYTES: [u8; 3000] = {
        let mut bytes = [0u8; 3000];
        let mut n = 0;
        while n < 1000 {
            bytes[3 * n] = b'0' + (n / 100) as u8;
            bytes[3 * n + 1] = b'0' + (n / 10 % 10) as u8;
            bytes[3 * n + 2] = b'0' + (n % 10) as u8;
            n += 1;
        }
        bytes
    };
    match std::str::from_utf8(&BYTES) {
        Ok(text) => text,
        Err(_) => panic!("ASCII digits"),
    }
};

/// `prefix` followed by the decimal text of `n`. Inlined, so that a
/// literal prefix is written as a constant-length copy.
#[inline]
pub(crate) fn write_decimal<W: Write + ?Sized>(out: &mut W, prefix: &str, n: i128) -> fmt::Result {
    if (0..1000).contains(&n) {
        if !prefix.is_empty() {
            out.write_str(prefix)?;
        }
        return out.write_str(small(n as usize));
    }
    write_large(out, prefix, n)
}

fn write_large<W: Write + ?Sized>(out: &mut W, prefix: &str, n: i128) -> fmt::Result {
    // Negative offsets and value numbers in big functions: a few pieces
    // of the tables rather than digits converted one at a time.
    let magnitude = n.unsigned_abs();
    if magnitude < 1_000_000 {
        let m = magnitude as usize;
        out.write_str(prefix)?;
        if n < 0 {
            out.write_char('-')?;
        }
        if m < 1000 {
            return out.write_str(small(m));
        }
        out.write_str(small(m / 1000))?;
        return out.write_str(&PADDED[3 * (m % 1000)..3 * (m % 1000) + 3]);
    }
    let mut buf = [0u8; 64];
    let mut i = buf.len();
    let mut magnitude = magnitude;
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
#[inline]
pub(crate) fn write_class<W: Write + ?Sized>(out: &mut W, class: char, n: i128) -> fmt::Result {
    out.write_char(class)?;
    write_decimal(out, "", n)
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

/// Assembly text being written. Most of it is appended a few bytes at a
/// time, so short pieces are copied inline rather than through `memcpy`;
/// everything else is the [`String`] it derefs to.
#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct Text(String);

impl Text {
    pub(crate) fn into_string(self) -> String {
        self.0
    }

    /// [`String::push_str`], with pieces of at most 16 bytes copied as two
    /// overlapping fixed-size words.
    #[inline(always)]
    pub(crate) fn push_str(&mut self, s: &str) {
        let len = s.len();
        if len > 16 {
            self.0.push_str(s);
            return;
        }
        self.0.reserve(len);
        let src = s.as_ptr();
        // SAFETY: `reserve` leaves room for `len` more bytes after the
        // current length. Each copy reads within `s` and writes within
        // those `len` bytes (the two words overlap when `s` is shorter than
        // both), and together they write every one of them, so the length
        // grows over exactly the bytes of `s`, which is valid UTF-8.
        unsafe {
            let vec = self.0.as_mut_vec();
            let dst = vec.as_mut_ptr().add(vec.len());
            if len >= 8 {
                let head = src.cast::<u64>().read_unaligned();
                let tail = src.add(len - 8).cast::<u64>().read_unaligned();
                dst.cast::<u64>().write_unaligned(head);
                dst.add(len - 8).cast::<u64>().write_unaligned(tail);
            } else if len >= 4 {
                let head = src.cast::<u32>().read_unaligned();
                let tail = src.add(len - 4).cast::<u32>().read_unaligned();
                dst.cast::<u32>().write_unaligned(head);
                dst.add(len - 4).cast::<u32>().write_unaligned(tail);
            } else if len > 0 {
                *dst = *src;
                *dst.add(len / 2) = *src.add(len / 2);
                *dst.add(len - 1) = *src.add(len - 1);
            }
            vec.set_len(vec.len() + len);
        }
    }
}

impl std::ops::Deref for Text {
    type Target = String;

    fn deref(&self) -> &String {
        &self.0
    }
}

impl std::ops::DerefMut for Text {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.0
    }
}

impl Write for Text {
    #[inline(always)]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }

    #[inline]
    fn write_char(&mut self, c: char) -> fmt::Result {
        self.0.push(c);
        Ok(())
    }
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
            -999,
            -1000,
            1000,
            1001,
            4095,
            -40960,
            999_999,
            1_000_000,
            -1_000_000,
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

    #[test]
    fn text_appends_every_length() {
        let piece = "0123456789abcdefghijklmnopqrstuvwxyzé";
        let (mut text, mut expected) = (Text::default(), String::new());
        for end in 0..=piece.len() {
            let Some(s) = piece.get(..end) else { continue };
            text.push_str(s);
            expected.push_str(s);
            write!(text, "{end}").unwrap();
            write!(expected, "{end}").unwrap();
        }
        assert_eq!(text.into_string(), expected);
    }
}
