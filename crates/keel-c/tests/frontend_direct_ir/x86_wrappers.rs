//! x86 intrinsic wrappers reach the optimizer as plain IR: a vector load or
//! store is one access with no stack copy or `memcpy`, and an unmasked
//! AVX-512 load is an ordinary load.
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
        assert!(!f.contains("intrinsic"), "{f}");
    }
}

