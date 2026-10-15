//! Validation-only benchmark. Map inputs before timing; rotate validators each
//! round. Usage: cargo run --release -p keel --example validate_input -- ROUNDS FILE...
use std::{hint::black_box, time::Instant};

type Validator = fn(&[u8]) -> Option<&str>;
const VALIDATORS: [(&str, Validator); 5] = [
    ("std", |b| std::str::from_utf8(b).ok()),
    ("ascii_then_std", |b| match ascii::AsciiStr::from_ascii(b) {
        Ok(s) => Some(s.as_str()),
        Err(_) => std::str::from_utf8(b).ok(),
    }),
    ("std_ascii_then_utf8", |b| {
        if b.is_ascii() {
            // SAFETY: every ASCII byte is a valid one-byte UTF-8 code point.
            Some(unsafe { std::str::from_utf8_unchecked(b) })
        } else {
            std::str::from_utf8(b).ok()
        }
    }),
    ("simdutf8", |b| simdutf8::basic::from_utf8(b).ok()),
    (
        "ascii_then_simdutf8",
        |b| match ascii::AsciiStr::from_ascii(b) {
            Ok(s) => Some(s.as_str()),
            Err(_) => simdutf8::basic::from_utf8(b).ok(),
        },
    ),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let rounds: usize = args.next().ok_or("expected ROUNDS FILE...")?.parse()?;
    let mut inputs = Vec::new();
    for path in args {
        let file = std::fs::File::open(&path)?;
        // SAFETY: benchmark fixtures stay unchanged for the duration of the run.
        let map = unsafe { memmap2::MmapOptions::new().map(&file)? };
        inputs.push((path, map));
    }
    let bytes: usize = inputs.iter().map(|(_, m)| m.len()).sum();
    if bytes == 0 || rounds == 0 {
        return Err("expected nonempty inputs and positive rounds".into());
    }
    let inner = (16 * 1024 * 1024 / bytes).max(1);
    for (_, map) in &inputs {
        let expected = std::str::from_utf8(map).ok();
        for (_, validate) in VALIDATORS {
            assert_eq!(validate(map), expected);
        }
    }
    let mut samples: [Vec<f64>; 5] = std::array::from_fn(|_| Vec::new());
    for round in 0..rounds + 3 {
        for offset in 0..VALIDATORS.len() {
            let index = (round + offset) % VALIDATORS.len();
            let start = Instant::now();
            for _ in 0..inner {
                for (_, map) in &inputs {
                    black_box(VALIDATORS[index].1(black_box(map)));
                }
            }
            let ms = start.elapsed().as_secs_f64() * 1000.0 / inner as f64;
            if round >= 3 {
                samples[index].push(ms);
            }
        }
    }
    println!(
        "{{\"bytes\":{bytes},\"inner\":{inner},\"files\":{:?},\"samples_ms\":{{",
        inputs.iter().map(|(p, _)| p).collect::<Vec<_>>()
    );
    for (i, (name, _)) in VALIDATORS.iter().enumerate() {
        println!(
            "{name:?}:{:?}{}",
            samples[i],
            if i + 1 == VALIDATORS.len() { "" } else { "," }
        );
    }
    println!("}}}}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validators_agree_at_simd_boundaries_and_on_invalid_bytes() {
        for length in 0..=257 {
            let mut bytes = vec![b'a'; length];
            for value in [0, 0x7f, 0x80, 0xc0, 0xc2, 0xe0, 0xf0, 0xff] {
                for offset in 0..length {
                    bytes[offset] = value;
                    for (_, validator) in VALIDATORS {
                        assert_eq!(validator(&bytes), std::str::from_utf8(&bytes).ok());
                    }
                    bytes[offset] = b'a';
                }
            }
            for text in ["λ", "中", "🦀", "\u{10ffff}"] {
                bytes.extend_from_slice(text.as_bytes());
                for (_, validator) in VALIDATORS {
                    assert_eq!(validator(&bytes), std::str::from_utf8(&bytes).ok());
                }
            }
        }
    }
}
