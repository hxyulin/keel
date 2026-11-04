//! LEB128 encoding: seven value bits a byte, low bits first, with the top bit
//! set on every byte but the last.

/// Longest encoding of a 128-bit value.
const MAX_LEN: usize = 128usize.div_ceil(7);

/// Append the minimal unsigned LEB128 encoding of `value`.
pub fn unsigned(out: &mut impl Extend<u8>, mut value: u128) {
    let mut bytes = [0; MAX_LEN];
    let mut len = 0;
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        let last = value == 0;
        bytes[len] = if last { byte } else { byte | 0x80 };
        len += 1;
        if last {
            break;
        }
    }
    out.extend(bytes[..len].iter().copied());
}

/// Append the minimal signed LEB128 encoding of `value`.
pub fn signed(out: &mut impl Extend<u8>, mut value: i128) {
    let mut bytes = [0; MAX_LEN];
    let mut len = 0;
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        let last = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        bytes[len] = if last { byte } else { byte | 0x80 };
        len += 1;
        if last {
            break;
        }
    }
    out.extend(bytes[..len].iter().copied());
}

/// Pad the minimal encoding at `out[start..]` to `width` bytes with
/// continuation bytes, as LLVM does: unsigned 127 in two bytes is `ff 00`.
/// `negative` selects the sign fill of a signed encoding. An encoding that
/// already fills `width` bytes is left alone.
///
/// # Panics
///
/// Panics if `out[start..]` is empty.
pub fn pad(out: &mut Vec<u8>, start: usize, width: usize, negative: bool) {
    let len = out.len() - start;
    if len >= width {
        return;
    }
    *out.last_mut().expect("an encoded value") |= 0x80;
    let fill = if negative { 0x7f } else { 0 };
    out.extend(std::iter::repeat_n(fill | 0x80, width - len - 1));
    out.push(fill);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(value: u128) -> Vec<u8> {
        let mut out = Vec::new();
        unsigned(&mut out, value);
        out
    }
    fn s(value: i128) -> Vec<u8> {
        let mut out = Vec::new();
        signed(&mut out, value);
        out
    }

    #[test]
    fn unsigned_boundaries() {
        assert_eq!(u(0), [0]);
        assert_eq!(u(127), [0x7f]);
        assert_eq!(u(128), [0x80, 0x01]);
        assert_eq!(u(624_485), [0xe5, 0x8e, 0x26]);
        assert_eq!(u(u64::MAX.into()).len(), 10);
        let max = u(u128::MAX);
        assert_eq!(max.len(), MAX_LEN);
        assert_eq!(max.last(), Some(&0x03));
    }

    #[test]
    fn signed_boundaries() {
        assert_eq!(s(0), [0]);
        assert_eq!(s(63), [0x3f]);
        assert_eq!(s(64), [0xc0, 0x00]);
        assert_eq!(s(-1), [0x7f]);
        assert_eq!(s(-64), [0x40]);
        assert_eq!(s(-65), [0xbf, 0x7f]);
        assert_eq!(s(-123_456), [0xc0, 0xbb, 0x78]);
        assert_eq!(s(i64::MIN.into()).len(), 10);
        assert_eq!(s(i128::MIN).len(), MAX_LEN);
        assert_eq!(s(i128::MAX).len(), MAX_LEN);
    }

    #[test]
    fn pad_keeps_value_and_reaches_width() {
        let mut out = vec![0xaa];
        unsigned(&mut out, 127);
        pad(&mut out, 1, 3, false);
        assert_eq!(out, [0xaa, 0xff, 0x80, 0x00]);
        let mut out = s(-1);
        pad(&mut out, 0, 2, true);
        assert_eq!(out, [0xff, 0x7f]);
        let mut out = u(300);
        pad(&mut out, 0, 1, false);
        assert_eq!(out, u(300));
    }

    #[test]
    fn extends_other_collections() {
        let mut out = std::collections::VecDeque::new();
        unsigned(&mut out, 300);
        assert_eq!(out, [0xac, 0x02]);
    }
}
