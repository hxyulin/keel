use serde_json::Value;
use std::{fs, process::Command};

fn invoke(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .args(args)
        .env("PATH", "")
        .output()
        .unwrap()
}

#[test]
fn exact_x86_metrics_need_no_external_tools_and_keep_a64_schema() {
    let dir = std::env::temp_dir().join(format!("keel-asm-metrics-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.s");
    let input = input.to_str().unwrap();
    let blob = dir.join("blob.bin");
    fs::write(&blob, [0x90, 0xeb, 0xfe]).unwrap();
    fs::write(input,format!("entry:\n vaddps zmm0,zmm1,zmm2\n jmp done\n.byte 0xeb,0xfe\n.p2align 4\ndone:\ncall external@PLT\nret\n.incbin \"{}\"\n",blob.display())).unwrap();
    let result = invoke(&[
        "asm-metrics",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--target-features",
        "avx512f",
        input,
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["target"], "x86_64-unknown-linux-gnu");
    let regions = report["regions"].as_array().unwrap();
    let instructions: Vec<_> = regions
        .iter()
        .filter(|r| r["kind"] == "instruction")
        .collect();
    assert_eq!(instructions.len(), 4);
    assert_eq!(instructions[0]["prefixes"][0], 0x62);
    assert!(instructions[1]["branch"]["displacement"].is_number());
    assert!(instructions[2]["branch"]["displacement"].is_null());
    assert!(
        regions
            .iter()
            .any(|r| r["kind"] == "data" && r["size"] == 2)
    );
    assert!(regions.iter().any(|r| r["kind"] == "alignment"));
    assert!(
        regions
            .iter()
            .any(|r| r["kind"] == "data" && r["size"] == 3)
    );
    let output = dir.join("exact.json");
    let saved = invoke(&[
        "asm-metrics",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--target-features",
        "avx512f",
        input,
        "-o",
        output.to_str().unwrap(),
    ]);
    assert!(saved.status.success());
    assert_eq!(fs::read(output).unwrap(), result.stdout);
    assert!(
        !invoke(&["asm-metrics", "--target", "x86_64-unknown-linux-gnu", input])
            .status
            .success()
    );
    assert!(
        !invoke(&[
            "asm-metrics",
            "--target",
            "aarch64-unknown-linux-gnu",
            input
        ])
        .status
        .success()
    );
    fs::write(input, "add x0,x0,#1\n.byte 0x90\n").unwrap();
    let old = invoke(&["asm-metrics", "--target", "aarch64-apple-darwin", input]);
    assert!(
        old.status.success(),
        "{}",
        String::from_utf8_lossy(&old.stderr)
    );
    let old: Value = serde_json::from_slice(&old.stdout).unwrap();
    assert_eq!(old["schema_version"], 1);
    assert_eq!(old["instruction_bytes"], 4);
    assert_eq!(old["text_bytes"], 5);
    assert!(
        !invoke(&[
            "asm-metrics",
            "--target",
            "aarch64-apple-darwin",
            "--target-features",
            "none",
            input
        ])
            .status
            .success()
    );
    fs::remove_dir_all(dir).unwrap();
}
