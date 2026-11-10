use super::{invoke, successful};
use std::{fmt::Write, fs, path::Path};

/// Objects and assembly do not depend on how many threads generate code.
#[test]
fn output_is_identical_for_every_codegen_thread_count() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("codegen-threads");
    fs::create_dir_all(&dir).unwrap();
    // Functions of different sizes, so workers finish out of order, with
    // calls between them, data, and constructors.
    let mut source = String::from("int table[64];\nconst char *names[] = {\"a\", \"b\"};\n");
    for i in 0..48 {
        let rounds = i % 7 * 3 + 1;
        writeln!(
            source,
            "int f{i}(int x, int *p) {{ int s = {i}; for (int k = 0; k < x; k++) {{ \
             switch ((k + s) % {rounds}) {{ case 0: s += p[k % 4]; break; case 1: s ^= k * {i}; break; \
             default: s -= table[k & 63]; }} }} return s{call}; }}",
            call = if i > 0 { format!(" + f{}(x - 1, p)", i - 1) } else { String::new() },
        )
        .unwrap();
    }
    source.push_str("__attribute__((constructor)) static void init(void) { table[1] = 2; }\n");
    fs::write(dir.join("input.c"), source).unwrap();
    for target in [
        "aarch64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
    ] {
        for flags in [&["-O0"][..], &["-O2"], &["-O2", "-g"]] {
            for object in [false, true] {
                let mut outputs = Vec::new();
                for threads in ["1", "3", "16"] {
                    let out = format!("out-{threads}");
                    let mut args = vec!["cc", "--target", target, "--codegen-threads", threads];
                    args.extend_from_slice(flags);
                    if object {
                        args.push("--emit-object");
                    }
                    args.extend(["input.c", "-o", &out]);
                    successful(&dir, &args);
                    outputs.push(fs::read(dir.join(&out)).unwrap());
                }
                assert!(
                    outputs.iter().all(|o| *o == outputs[0]),
                    "{target} {flags:?} object {object}: output depends on thread count"
                );
            }
        }
    }
    let rejected = invoke(&dir, &["cc", "--codegen-threads", "0", "input.c"]);
    assert!(!rejected.status.success());
}
