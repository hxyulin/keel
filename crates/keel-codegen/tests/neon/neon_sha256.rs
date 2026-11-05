//! AArch64 SHA-256 intrinsics: a full block compression built from them must
//! match a scalar C SHA-256 and the FIPS 180-2 "abc" digest.
use crate::common;
use keel_codegen::{
    CodegenPipeline, SelectionTier, Target,
    regalloc::{AllocationTier, Pools},
};
use std::{fs, process::Command};

const SPILL: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

/// `@block(state, words, k)`: one SHA-256 compression. `state` holds the
/// eight hash words, `words` the sixteen message words already decoded from
/// big-endian, and `k` the 64 round constants.
fn block_ir() -> String {
    let mut ir = String::from("func @block(ptr, ptr, ptr) {\nblock0(v0: ptr, v1: ptr, v2: ptr):\n");
    let mut next = 3;
    let mut value = |ir: &mut String, text: String| {
        let v = format!("v{next}");
        next += 1;
        *ir += &format!(" {v} = {text}\n");
        v
    };
    let mut load = |ir: &mut String, base: &str, offset: usize| {
        let address = if offset == 0 {
            base.to_string()
        } else {
            let o = value(ir, format!("iconst.i64 {offset}"));
            value(ir, format!("ptradd {base}, {o}"))
        };
        value(ir, format!("load.i32x4 align 4 {address}"))
    };
    let s0 = load(&mut ir, "v0", 0);
    let s1 = load(&mut ir, "v0", 16);
    let mut w: Vec<String> = (0..4).map(|j| load(&mut ir, "v1", 16 * j)).collect();
    let k: Vec<String> = (0..16).map(|i| load(&mut ir, "v2", 16 * i)).collect();
    let (mut abcd, mut efgh) = (s0.clone(), s1.clone());
    for i in 0..16 {
        let wk = value(&mut ir, format!("viadd {}, {}", w[i % 4], k[i]));
        let h = value(
            &mut ir,
            format!("intrinsic aarch64_sha256h({abcd}, {efgh}, {wk})"),
        );
        efgh = value(
            &mut ir,
            format!("intrinsic aarch64_sha256h2({efgh}, {abcd}, {wk})"),
        );
        abcd = h;
        if i < 12 {
            let t = value(
                &mut ir,
                format!(
                    "intrinsic aarch64_sha256su0({}, {})",
                    w[i % 4],
                    w[(i + 1) % 4]
                ),
            );
            w[i % 4] = value(
                &mut ir,
                format!(
                    "intrinsic aarch64_sha256su1({t}, {}, {})",
                    w[(i + 2) % 4],
                    w[(i + 3) % 4]
                ),
            );
        }
    }
    let out0 = value(&mut ir, format!("viadd {abcd}, {s0}"));
    let out1 = value(&mut ir, format!("viadd {efgh}, {s1}"));
    let o = value(&mut ir, "iconst.i64 16".into());
    let high = value(&mut ir, format!("ptradd v0, {o}"));
    ir += &format!(" store align 4 {out0}, v0\n store align 4 {out1}, {high}\n return\n}}\n");
    ir
}

const ORACLE: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
void block(uint32_t *state, const uint32_t *words, const uint32_t *k);
static const uint32_t K[64] = {
0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2};
static uint32_t rotr(uint32_t x, int n) { return x >> n | x << (32 - n); }
static void scalar(uint32_t *h, const uint32_t *m) {
    uint32_t w[64], s[8];
    for (int i = 0; i < 16; i++) w[i] = m[i];
    for (int i = 16; i < 64; i++)
        w[i] = w[i-16] + (rotr(w[i-15],7) ^ rotr(w[i-15],18) ^ w[i-15] >> 3) + w[i-7]
             + (rotr(w[i-2],17) ^ rotr(w[i-2],19) ^ w[i-2] >> 10);
    memcpy(s, h, sizeof s);
    for (int i = 0; i < 64; i++) {
        uint32_t t1 = s[7] + (rotr(s[4],6) ^ rotr(s[4],11) ^ rotr(s[4],25))
                    + ((s[4] & s[5]) ^ (~s[4] & s[6])) + K[i] + w[i];
        uint32_t t2 = (rotr(s[0],2) ^ rotr(s[0],13) ^ rotr(s[0],22))
                    + ((s[0] & s[1]) ^ (s[0] & s[2]) ^ (s[1] & s[2]));
        memmove(s + 1, s, 7 * sizeof *s);
        s[4] += t1;
        s[0] = t1 + t2;
    }
    for (int i = 0; i < 8; i++) h[i] += s[i];
}
int main(void) {
    /* FIPS 180-2 "abc": one padded block. */
    uint32_t words[16] = {0x61626380, [15] = 24}, state[8] = {
        0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19};
    static const uint32_t abc[8] = {
        0xba7816bf,0x8f01cfea,0x414140de,0x5dae2223,0xb00361a3,0x96177a9c,0xb410ff61,0xf20015ad};
    block(state, words, K);
    if (memcmp(state, abc, sizeof abc)) { fprintf(stderr, "abc digest\n"); return 1; }
    uint64_t seed = 0x123456789abcdefull;
    for (int round = 0; round < 1000; round++) {
        uint32_t expected[8];
        for (int i = 0; i < 16; i++) { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; words[i] = (uint32_t)seed; }
        for (int i = 0; i < 8; i++) { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; state[i] = (uint32_t)(seed >> 32); }
        memcpy(expected, state, sizeof state);
        scalar(expected, words);
        block(state, words, K);
        if (memcmp(state, expected, sizeof state)) { fprintf(stderr, "round %d\n", round); return 1; }
    }
    return 0;
}
"#;

#[test]
fn sha256_block_matches_scalar_sha256_across_selection_allocation_and_spills() {
    let module = keel_ir::parse(&block_ir()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let features = keel_asm::TargetFeatures::default().with_sha256();
    let dir = common::temp("neon-sha256");
    fs::write(dir.join("oracle.c"), ORACLE).unwrap();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    common::command(
        Command::new("clang")
            .args(["-O2", "-c"])
            .arg(dir.join("oracle.c"))
            .arg("-o")
            .arg(dir.join("oracle.o")),
    );
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::aarch64::REGISTERS, &SPILL] {
                let mut m = module.clone();
                if selector == SelectionTier::Full {
                    keel_opt::optimize(&mut m).unwrap();
                }
                let p = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text = keel_codegen::aarch64::compile_with_pipeline(&m, pools, &p).unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &p).unwrap();
                let direct = keel_asm::assemble_program_object(&program, features).unwrap();
                let parsed = keel_asm::assemble_object_with_features(&text, features).unwrap();
                assert_eq!(direct, parsed);
                assert!(
                    keel_asm::assemble_object(&text).is_err(),
                    "needs FEAT_SHA256"
                );
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                {
                    fs::write(dir.join("test.o"), direct).unwrap();
                    common::command(
                        Command::new("clang")
                            .arg(dir.join("oracle.o"))
                            .arg(dir.join("test.o"))
                            .arg("-o")
                            .arg(dir.join("test")),
                    );
                    common::command(&mut Command::new(dir.join("test")));
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sha256_intrinsics_are_rejected_without_feat_sha256() {
    let module = keel_ir::parse(
        "func @f(ptr) {\nblock0(v0: ptr):\n v1 = load.i32x4 align 4 v0\n v2 = intrinsic aarch64_sha256su0(v1, v1)\n store align 4 v2, v0\n return\n}\n",
    )
    .unwrap();
    keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
    for (target, message) in [
        (Target::Aarch64LinuxGnu, "requires FEAT_SHA256"),
        (Target::X86_64LinuxGnu, "is an AArch64 intrinsic"),
    ] {
        let error = keel_codegen::compile(&module, target)
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{target:?}: {error}");
    }
}
