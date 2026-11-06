//! x86 intrinsic wrappers reach the optimizer as plain IR: a vector load or
//! store is one access with no stack copy or `memcpy`, an unmasked AVX-512
//! load is an ordinary load, byte shifts are one constant shuffle, and a
//! constant AVX-512 shift count uses the immediate intrinsic.
use super::{body, count};
use keel_ir::Target;

fn x86_ir(src: &str) -> String {
    let target = Target::X86_64LinuxGnu;
    let src = keel_c::preprocess(
        &format!("#include <immintrin.h>\n{src}"),
        "wrappers.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    let module = keel_c::compile(
        &src,
        "wrappers.c",
        keel_c::Options {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module.to_string()
}

#[test]
fn unaligned_vector_loads_are_one_load() {
    let ir = x86_ir(
        "__m128 ps(const float *p) { return _mm_loadu_ps(p); }\n\
         __m128d pd(const double *p) { return _mm_loadu_pd(p); }\n\
         __m128i si(const void *p) { return _mm_loadu_si128((const __m128i_u *)p); }\n\
         __m256 ps256(const float *p) { return _mm256_loadu_ps(p); }\n\
         __m256i si256(const void *p) { return _mm256_lddqu_si256((const __m256i_u *)p); }\n\
         __m512 ps512(const void *p) { return _mm512_loadu_ps(p); }\n\
         __m512i si512(const void *p) { return _mm512_loadu_si512(p); }\n\
         __m512i epi8(const void *p) { return _mm512_loadu_epi8(p); }",
    );
    for name in ["ps", "pd", "si", "ps256", "si256", "ps512", "si512", "epi8"] {
        let f = body(&ir, name);
        assert_eq!(count(f, "load"), 1, "{f}");
        assert!(!f.contains("stack_slot") && !f.contains("intrinsic"), "{f}");
    }
}

#[test]
fn vector_stores_and_scalar_accesses_use_no_memcpy() {
    let ir = x86_ir(
        "void ps(float *q, const float *p) { _mm_storeu_ps(q, _mm_loadu_ps(p)); }\n\
         void si512(void *q, const void *p) { _mm512_storeu_si512(q, _mm512_loadu_si512(p)); }\n\
         float ss(const float *p) { return _mm_cvtss_f32(_mm_load_ss(p)); }\n\
         int si32(const void *p) { return _mm_cvtsi128_si32(_mm_loadu_si32(p)); }\n\
         void sd(double *q, __m128d a) { _mm_storeh_pd(q, a); }",
    );
    for name in ["ps", "si512", "ss", "si32", "sd"] {
        let f = body(&ir, name);
        // The inliner bounds each inlined snapshot with lifetime markers.
        assert!(
            !f.lines()
                .any(|l| l.contains("intrinsic") && !l.contains("intrinsic lifetime_")),
            "{f}"
        );
    }
}

#[test]
fn byte_shifts_and_alignr_are_one_shuffle() {
    let ir = x86_ir(
        "__m128i sll(__m128i a) { return _mm_slli_si128(a, 4); }\n\
         __m128i srl(__m128i a) { return _mm_srli_si128(a, 15); }\n\
         __m128i align(__m128i a, __m128i b) { return _mm_alignr_epi8(a, b, 20); }\n\
         __m256i sll256(__m256i a) { return _mm256_slli_si256(a, 3); }\n\
         __m256i align256(__m256i a, __m256i b) { return _mm256_alignr_epi8(a, b, 5); }",
    );
    for (name, indices) in [
        (
            "sll",
            "[0, 1, 2, 3, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27]",
        ),
        (
            "srl",
            "[15, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31]",
        ),
        (
            "align",
            "[4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19]",
        ),
    ] {
        let f = body(&ir, name);
        assert_eq!(count(f, "vshuffle"), 1, "{f}");
        assert!(f.contains(indices), "{f}");
        assert!(!f.contains("brif") && !f.contains("intrinsic"), "{f}");
    }
    for name in ["sll256", "align256"] {
        let f = body(&ir, name);
        assert_eq!(count(f, "vshuffle"), 1, "{f}");
        assert!(!f.contains("brif") && !f.contains("intrinsic"), "{f}");
    }
}

#[test]
fn avx512_constant_shift_counts_use_immediate_intrinsics() {
    let ir = x86_ir(
        "__m512i d(__m512i a) { return _mm512_slli_epi32(a, 3); }\n\
         __m512i q(__m512i s, __mmask8 k, __m512i a) { return _mm512_mask_srai_epi64(s, k, a, 255); }\n\
         __m256i vl(__mmask8 k, __m256i a) { return _mm256_maskz_srli_epi32(k, a, 31); }\n\
         __m128i w(__m128i a) { return _mm_mask_srai_epi16(a, 3, a, 1); }\n\
         __m512i w512(__m512i a) { return _mm512_srli_epi16(a, 16); }\n\
         __m512i big(__m512i a) { return _mm512_slli_epi32(a, 256); }\n\
         __m512i neg(__m512i a) { return _mm512_srai_epi64(a, -1); }\n\
         __m512i run(__m512i a, unsigned n) { return _mm512_slli_epi32(a, n); }",
    );
    for (name, intrinsic) in [
        ("d", "x86_avx512_masksllid("),
        ("q", "x86_avx512_masksraiq("),
        ("vl", "x86_avx512vl_srlid256("),
        ("w", "x86_avx512bw_sraiw128("),
        ("w512", "x86_avx512bw_srliw512("),
        ("big", "x86_avx512_maskslld("),
        ("neg", "x86_avx512_masksraq("),
        ("run", "x86_avx512_maskslld("),
    ] {
        let f = body(&ir, name);
        assert_eq!(count(f, "intrinsic"), 1, "{f}");
        assert!(f.contains(intrinsic), "{f}");
    }
}
