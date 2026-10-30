#[path = "../../../keel-codegen/tests/common/avx512_vl_integer.rs"]
mod common;
use super::pclmul::parse;
#[test]
fn vl_integer_standard_wrappers_match_scalar_and_system_headers() {
    let (_, mut source, mut oracle) = common::fixtures();
    for width in [128, 256] {
        let (pre, v, lanes) = if width == 128 {
            ("_mm", "__m128i", 2)
        } else {
            ("_mm256", "__m256i", 4)
        };
        let initializers = vec!["0x1234"; lanes].join(",");
        source += &format!(
            "static int vc{width},mc{width};static __attribute__((always_inline,target(\"avx512vl\"))) {v} next{width}(void){{vc{width}++;return ({v}){{{initializers}}};}}static int mask{width}(void){{mc{width}++;return 255;}}__attribute__((target(\"avx512vl\"))) int eval{width}(void){{{v} r;unsigned long long values[{lanes}];"
        );
        for (mode, expr, maskcalls) in [
            (
                0,
                format!(
                    "{pre}_mask_ternarylogic_epi32(next{width}(),mask{width}(),next{width}(),next{width}(),0x96)"
                ),
                1,
            ),
            (
                1,
                format!(
                    "{pre}_maskz_ternarylogic_epi32(mask{width}(),next{width}(),next{width}(),next{width}(),0x96)"
                ),
                1,
            ),
            (
                2,
                format!("{pre}_ternarylogic_epi32(next{width}(),next{width}(),next{width}(),0x96)"),
                0,
            ),
        ] {
            source += &format!(
                "vc{width}=mc{width}=0;r={expr};__builtin_memcpy(values,&r,sizeof r);if(vc{width}!=3||mc{width}!={maskcalls}||values[0]!=0x1234)return {mode}+1;"
            );
        }
        source += "return 0;}\n";
    }
    oracle=oracle.replace("int main(void){","extern int eval128(void);extern int eval256(void);int main(void){if(eval128()||eval256())return 9;");
    let mut module = parse(&source, keel_ir::Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    common::check(&module, "vl-integer-c", Some(&oracle));
}
#[test]
fn vl_integer_ternary_controls_and_missing_unmasked_aliases() {
    for width in [128, 256] {
        let (pre, v) = if width == 128 {
            ("_mm", "__m128i")
        } else {
            ("_mm256", "__m256i")
        };
        let mut source = String::from("#include <immintrin.h>\n");
        for (i, name) in [
            "min_epi64",
            "min_epu64",
            "max_epi64",
            "max_epu64",
            "and_epi32",
            "and_epi64",
            "andnot_epi32",
            "andnot_epi64",
            "or_epi32",
            "or_epi64",
            "xor_epi32",
            "xor_epi64",
            "abs_epi64",
        ]
        .iter()
        .enumerate()
        {
            let args = if *name == "abs_epi64" { "a" } else { "a,b" };
            source += &format!(
                "__attribute__((target(\"avx512vl\"))) {v} alias{i}({v} a,{v} b){{return {pre}_{name}({args});}}\n"
            );
        }
        let module = parse(&source, keel_ir::Target::X86_64LinuxGnu).unwrap();
        keel_codegen::compile_program(&module, keel_ir::Target::X86_64LinuxGnu).unwrap();
        for imm in ["-1", "256", "runtime"] {
            let source = format!(
                "#include <immintrin.h>\n{v} f({v} a,{v} b,{v} c,int runtime){{return {pre}_ternarylogic_epi32(a,b,c,{imm});}}"
            );
            assert!(
                keel_c::compile(
                    &source,
                    "bad.c",
                    keel_c::Options {
                        target: keel_ir::Target::X86_64LinuxGnu,
                        debug_info: false
                    }
                )
                .is_err(),
                "{imm}"
            );
        }
    }
}
