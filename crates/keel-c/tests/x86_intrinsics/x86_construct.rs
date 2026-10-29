use crate::packet;

#[test]
fn constructors_broadcasts_and_duplicate_moves_preserve_bits() {
    packet::check(
        include_str!("../fixtures/x86_construct.c"),
        include_str!("../fixtures/x86_construct_runner.c"),
        "KEEL_C_CONSTRUCT_EXPORT_DIR",
        "construct",
        None,
    );
}

#[test]
fn insert_ps_requires_a_constant_control() {
    let source = keel_c::preprocess(
        "#include <immintrin.h>\n__m128 f(__m128 a,__m128 b,int i){return _mm_insert_ps(a,b,i);}",
        "bad.c",
        &keel_c::PreprocessOptions {
            target: keel_ir::Target::X86_64LinuxGnu,
            ..Default::default()
        },
    )
    .unwrap();
    let error = keel_c::compile(
        &source,
        "bad.c",
        keel_c::Options {
            target: keel_ir::Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("constant"), "{error}");
}

#[test]
fn unaligned_vector_aliases_preserve_size_and_pointer_admission() {
    let mut source = String::from("#include <immintrin.h>\n");
    for (name, bytes) in [
        ("__m128", 16),
        ("__m128d", 16),
        ("__m128i", 16),
        ("__m256", 32),
        ("__m256d", 32),
        ("__m256i", 32),
    ] {
        source.push_str(&format!(
            "_Static_assert(sizeof({name}_u)=={bytes},\"size\");\n\
             _Static_assert(_Alignof({name}_u)==1,\"unaligned typedef\");\n\
             _Static_assert(_Alignof({name})=={bytes},\"container alignment\");\n\
             _Static_assert(sizeof({name}_u[2])=={},\"array stride\");\n\
             struct layout_{name}{{char head;{name}_u value;char tail;}};\n\
             _Static_assert(sizeof(struct layout_{name})=={},\"packed member layout\");\n\
             _Static_assert(__builtin_offsetof(struct layout_{name},value)==1,\"member alignment\");\n\
             _Static_assert(__builtin_types_compatible_p({name},{name}_u),\"compatible lane representation\");\n",
            bytes*2,bytes+2,
        ));
    }
    source.push_str("__m128i copy128(const __m128i_u*p){return _mm_loadu_si128(p);}\n\
                     void write128(__m128i_u*p,__m128i a){_mm_storeu_si128(p,a);}\n\
                     __m256i copy256(const __m256i_u*p){return _mm256_lddqu_si256(p);}\n\
                     void write256(__m256i_u*p,__m256i a){_mm256_storeu_si256(p,a);}\n\
                     __m256i split(const __m128i_u*hi,const __m128i_u*lo){return _mm256_loadu2_m128i(hi,lo);}\n");
    packet::frontend(&source);
}
