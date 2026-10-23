use keel_asm_ir::x86::{Features, Instruction, required_features};

#[test]
fn borrowed_requirements_preserve_form_specific_contracts_and_fallbacks() {
    // Expected feature names are independent of either query implementation.
    for (source, expected) in [
        ("nop", ""),
        ("lock add dword ptr [rax],1", ""),
        ("vpaddd xmm0,xmm1,xmm2", "avx"),
        ("vpaddd ymm0,ymm1,ymm2", "avx,avx2"),
        ("vaddps ymm0,ymm1,ymm2", "avx"),
        ("vbroadcastss ymm0,dword ptr [rax]", "avx"),
        ("vbroadcastss ymm0,xmm1", "avx,avx2"),
        ("vbroadcastsd ymm0,qword ptr [rax]", "avx"),
        ("vbroadcastsd ymm0,xmm1", "avx,avx2"),
        ("vfmadd132ss xmm0,xmm1,xmm2", "avx,fma"),
        ("vfmadd132ps ymm0,ymm1,ymm2", "avx,fma"),
        ("vcvtph2ps xmm0,qword ptr [rax]", "avx,f16c"),
        ("vcvtph2ps ymm0,xmm1", "avx,f16c"),
        ("andn rax,rcx,rdx", "bmi1"),
        ("pext rax,rcx,rdx", "bmi2"),
        ("pclmulqdq xmm0,xmm1,255", "pclmulqdq"),
        ("aesenc xmm0,xmm1", "aes"),
        ("aeskeygenassist xmm0,xmmword ptr [rax],255", "aes"),
        ("aesenc ymm0,ymm1", "aes"),
        // Invalid forms retain the opcode fallback; this query isn't validation.
        ("vbroadcastss ymm0,rax", "avx"),
        ("vpaddd ymm0", "avx"),
        ("vaddps ymm0,ymm1,word ptr [rax]", "avx"),
    ] {
        let mut i = Instruction::parse_intel(source).unwrap();
        let mut want = Features::NONE;
        for name in expected.split(',').filter(|s| !s.is_empty()) {
            match name {
                "avx" => want.avx = true,
                "avx2" => want.avx2 = true,
                "fma" => want.fma = true,
                "f16c" => want.f16c = true,
                "bmi1" => want.bmi1 = true,
                "bmi2" => want.bmi2 = true,
                "pclmulqdq" => want.pclmulqdq = true,
                "aes" => want.aes = true,
                _ => unreachable!(),
            }
        }
        assert_eq!(required_features(i.opcode, &i.operands), want, "{source}");
        assert_eq!(i.required_features(), want, "{source}");
        // Lock is deliberately outside feature/form selection, as before.
        i.lock = !i.lock;
        assert_eq!(i.required_features(), want, "{source}");
    }
}
