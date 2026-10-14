//! The two Advanced SIMD forms used by Keel's scalar popcount lowering.
use super::count;
use crate::encode;

fn number(text: &str, prefix: char) -> Result<u32, String> {
    let digits = text
        .strip_prefix(prefix)
        .ok_or("invalid SIMD register class")?;
    let number = digits
        .parse::<u32>()
        .map_err(|_| format!("invalid SIMD register `{text}`"))?;
    if number > 31 {
        return Err(format!("invalid SIMD register `{text}`"));
    }
    Ok(number)
}
fn vector(text: &str) -> Result<(u32, &str, u32, u32), String> {
    let (name, arrangement) = text
        .split_once('.')
        .ok_or("vector register requires an arrangement")?;
    let register = number(name, 'v')?;
    let (q, size) = match arrangement {
        "8b" => (0, 0),
        "16b" => (1, 0),
        "4h" => (0, 1),
        "8h" => (1, 1),
        "4s" => (1, 2),
        _ => return Err("unsupported SIMD reduction arrangement".into()),
    };
    Ok((register, arrangement, q, size))
}
pub(super) fn instruction(op: &str, args: &[&str]) -> Result<u32, String> {
    count(args, 2, 2)?;
    let d = args[0].to_ascii_lowercase();
    let n = args[1].to_ascii_lowercase();
    let (rn, arrangement, q, size) = vector(&n)?;
    let rd = if op == "cnt" {
        let (rd, dest_arrangement, _, _) = vector(&d)?;
        if size != 0 || arrangement != dest_arrangement {
            return Err("cnt requires matching 8B or 16B arrangements".into());
        }
        rd
    } else {
        number(
            &d,
            match size {
                0 => 'b',
                1 => 'h',
                _ => 's',
            },
        )?
    };
    encode(
        if op == "cnt" {
            "CNT_asimdmisc_R"
        } else {
            "ADDV_asimdall_only"
        },
        &[("Rd", rd), ("Rn", rn), ("Q", q), ("size", size)],
    )
}
