//! Bounded wrapping reductions: selection/encoding and exported native oracle.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier, Target,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
    x86_features::FeatureSelector,
};
use std::{fs, path::PathBuf};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const TINY: Pools = Pools {
    caller_saved: &[6],
    callee_saved: &[3],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
// Force same-number GPR/XMM views and shared spill homes independently of
// allocator heuristics. The vector dies before the scalar definition.
struct Controlled(u8);
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "reduction-controlled"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        if input.func.name.starts_with("sum_") {
            let (source, dest) = match self.0 {
                0 => (Loc::FReg(6), Loc::Reg(6)),
                1 => (Loc::Stack(0), Loc::Reg(6)),
                2 => (Loc::FReg(0), Loc::Stack(0)),
                3 => (Loc::Stack(0), Loc::Stack(0)),
                n => (Loc::FReg(13 + n - 4), Loc::Reg(6)),
            };
            a.spill_slots = 2;
            a.locs[0][0] = Some(source);
            a.locs[0][1] = Some(dest);
        }
        a
    }
}
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut main = String::from(
        "int main(void){uint64_t rng=17;long page=sysconf(_SC_PAGESIZE);unsigned char *map=mmap(0,3*page,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(map==MAP_FAILED)return 90;if(mprotect(map+page,page,PROT_READ|PROT_WRITE))return 91;\n",
    );
    for (shape, lane, bits, n) in [("i64x2", "i64", 64, 2), ("i32x4", "i32", 32, 4)] {
        // Scalar parameter is live until after reduction; vector remains live
        // across a call and after both reductions. Both extensions are explicit.
        ir += &format!(
            "decl @opaque_{shape}({shape}) -> {shape}\nfunc @sum_{shape}({shape}) -> {lane} {{\nblock0(v0: {shape}):\n v1 = vreduce_add v0\n return v1\n}}\nfunc @live_{shape}(ptr,ptr,i64) -> i64 {{\nblock0(v0: ptr,v1: ptr,v2: i64):\n v3 = load.{shape} align 1 v0\n v4 = vreduce_add v3\n v5 = call @opaque_{shape}(v3)\n v6 = vreduce_add v5\n store align 1 v4,v1\n v7 = iconst.i64 8\n v8 = ptradd v1,v7\n store align 1 v6,v8\n v9 = iconst.i64 16\n v10 = ptradd v1,v9\n store align 1 v3,v10\n"
        );
        if bits == 32 {
            ir += " v11 = sext.i64 v4\n v12 = uext.i64 v6\n v13 = iadd v11,v12\n";
        } else {
            ir += " v13 = iadd v4,v6\n";
        }
        ir += " v14 = iadd v13,v2\n return v14\n}\n";
        c += &format!(
            "typedef uint{bits}_t V_{shape} __attribute__((vector_size(16)));\nextern uint{bits}_t sum_{shape}(V_{shape});extern uint64_t live_{shape}(const void*,void*,uint64_t);\n__attribute__((noinline)) V_{shape} opaque_{shape}(V_{shape} x){{__asm__ volatile(\"pxor %%xmm15,%%xmm15\"::: \"xmm15\");return x;}}\n"
        );
        main += &format!(
            "for(unsigned k=0;k<(1u<<(3*{n}))+4096;k++){{uint{bits}_t a[{n}],sum=0;for(unsigned l=0;l<{n};l++){{rng^=rng<<13;rng^=rng>>7;rng^=rng<<17;uint64_t edge[]={{0,1,UINT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x7fffffffffffffff),UINT64_C(0x80000000),UINT64_C(0x7fffffff),UINT64_C(0xffffffff)}};a[l]=(uint{bits}_t)(k<(1u<<(3*{n}))?edge[(k>>(3*l))%8]:rng);if(k>=(1u<<(3*{n}))&&k<(1u<<(3*{n}))+{n})a[l]=(l==k-(1u<<(3*{n})))?(uint{bits}_t)-1:0;sum+=a[l];}}V_{shape} v;memcpy(&v,a,16);if(sum_{shape}(v)!=sum)return 1;for(unsigned edge=0;edge<3;edge++){{unsigned char *p=edge==0?map+page:edge==1?map+2*page-16:map+page+1;memcpy(p,a,16);unsigned char out[34],expected[34];memset(out,0xa5,34);memset(expected,0xa5,34);memcpy(expected+1,&sum,{bits}/8);memcpy(expected+9,&sum,{bits}/8);memcpy(expected+17,a,16);uint64_t got=live_{shape}(p,out+1,UINT64_C(0xfedcba9876543210));uint64_t e=UINT64_C(0xfedcba9876543210)+(uint64_t)({signed})sum+(uint64_t)sum;if(got!=e||memcmp(out,expected,34)||memcmp(p,a,16))return 2;}}}}\n",
            signed = if bits == 32 { "int32_t" } else { "uint64_t" }
        );
    }
    (ir, c + &main + "munmap(map,3*page);return 0;}\n")
}
#[test]
fn wrapping_add_paths_and_oracles() {
    let (ir, c) = fixture();
    let m = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let dir = std::env::var_os("KEEL_REDUCE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-reduce-{}", std::process::id()))
        });
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("oracle.c"), &c).unwrap();
    fs::write(dir.join("input.keel"), &ir).unwrap();
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for (alloc_name, alloc) in [
                ("Fast", &AllocationTier::Fast as &dyn RegisterAllocator),
                ("Full", &AllocationTier::Full),
                ("views", &Controlled(0)),
                ("inputspill", &Controlled(1)),
                ("resultspill", &Controlled(2)),
                ("sharedspill", &Controlled(3)),
            ] {
                for (label, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("tiny", &TINY),
                    ("empty", &EMPTY),
                ] {
                    if alloc_name != "Fast" && alloc_name != "Full" && label != "normal" {
                        continue;
                    }
                    let features = Features::for_level(level);
                    let selector = FeatureSelector {
                        inner: &sel,
                        features,
                    };
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: alloc,
                    };
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                    let p =
                        keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)
                            .unwrap();
                    assert_eq!(p.stats().text_bytes, 0);
                    let obj =
                        keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap();
                    assert_eq!(
                        obj,
                        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                    );
                    {
                        assert_eq!(text.matches("pshufd\t").count(), 9, "{text}");
                        assert_eq!(text.matches("paddq\t").count(), 3);
                        assert_eq!(text.matches("paddd\t").count(), 6);
                        let sum = text
                            .split("\"sum_i32x4\":")
                            .nth(1)
                            .unwrap()
                            .split(".size")
                            .next()
                            .unwrap();
                        if label == "normal" && matches!(alloc_name, "Fast" | "Full") {
                            assert!(!sum.contains("ptr ["), "{sum}");
                        }
                    }
                    let stem = format!("{level:?}-{sel:?}-{alloc_name}-{label}");
                    fs::write(dir.join(format!("{stem}.o")), &obj).unwrap();
                    fs::write(dir.join(format!("{stem}.s")), &text).unwrap();
                    if label == "normal" {
                        let mut chunks = keel_asm_ir::Program::default();
                        keel_codegen::compile_program_chunks_with_pipeline(
                            &m,
                            Target::X86_64LinuxGnu,
                            &pipeline,
                            &mut |part| {
                                chunks.items.extend(part.items);
                            },
                        )
                        .unwrap();
                        assert_eq!(
                            obj,
                            keel_asm::x86::assemble_program_object_with_features(&chunks, features)
                                .unwrap()
                        );
                    }
                    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
                    {
                        let exe = dir.join("native");
                        assert!(
                            std::process::Command::new("clang")
                                .args(["-O2", "-msse2", "-mno-avx"])
                                .arg(dir.join("oracle.c"))
                                .arg(dir.join(format!("{stem}.o")))
                                .arg("-o")
                                .arg(&exe)
                                .status()
                                .unwrap()
                                .success()
                        );
                        assert!(std::process::Command::new(exe).status().unwrap().success());
                    }
                }
            }
        }
    }
    // ARM control: shared lane reduction behavior still compiles identically on
    // text and typed paths. Existing ARM native vector suite covers semantics.
    let arm = keel_codegen::aarch64::compile(&m).unwrap();
    fs::write(dir.join("arm-control.s"), &arm).unwrap();
    let ap = keel_codegen::compile_program_with_pipeline(
        &m,
        Target::Aarch64AppleDarwin,
        &CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap();
    assert_eq!(
        keel_asm::assemble_object(&arm).unwrap(),
        keel_asm::assemble_program_object(&ap, Default::default()).unwrap()
    );
    if std::env::var_os("KEEL_REDUCE_EVIDENCE").is_none() {
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn effective_features_and_excluded_reductions() {
    let mut source = String::new();
    for (shape, lane, op) in [
        ("i8x16", "i8", "add"),
        ("i16x8", "i16", "add"),
        ("i64x2", "i64", "smin"),
        ("i32x4", "i32", "umax"),
        ("i64x4", "i64", "add"),
        ("i32x8", "i32", "add"),
    ] {
        let (result, finish) = if matches!(lane, "i8" | "i16") {
            ("i32", " v2 = uext.i32 v1\n return v2\n")
        } else {
            (lane, " return v1\n")
        };
        source += &format!(
            "func @control_{shape}({shape}) -> {result} {{\nblock0(v0: {shape}):\n v1 = vreduce_{op} v0\n{finish}}}\n"
        );
    }
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let mut features = Features::for_level(CpuLevel::V3);
    features.avx2 = false;
    features.avx = false;
    features.fma = false;
    features.f16c = false;
    for features in [
        Features::for_level(CpuLevel::V1),
        features,
        Features::for_level(CpuLevel::V3),
    ] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let mut selected = m.clone();
        if !features.avx {
            selected
                .functions
                .retain(|f| !f.name.ends_with("i64x4") && !f.name.ends_with("i32x8"));
        }
        let text =
            keel_codegen::compile_with_pipeline(&selected, Target::X86_64LinuxGnu, &pipeline)
                .unwrap();
        let p = keel_codegen::compile_program_with_pipeline(
            &selected,
            Target::X86_64LinuxGnu,
            &pipeline,
        )
        .unwrap();
        assert_eq!(
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
            keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap()
        );
        // No excluded shape/op may accidentally reach the new SSE add recipe.
        assert!(!text.contains("\tpshufd\t"));
        if let Some(dir) = std::env::var_os("KEEL_REDUCE_EVIDENCE") {
            let dir = PathBuf::from(dir);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join(format!(
                    "excluded-avx{}-popcnt{}.s",
                    features.avx, features.popcnt
                )),
                &text,
            )
            .unwrap();
        }
    }
    let eligible = keel_ir::parse(
        "func @sum(i32x4) -> i32 {\nblock0(v0: i32x4):\n v1 = vreduce_add v0\n return v1\n}\n",
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    let text = keel_codegen::compile_with_pipeline(
        &eligible,
        Target::X86_64LinuxGnu,
        &CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap();
    assert_eq!(text.matches("pshufd\t").count(), 2);
    assert_eq!(text.matches("paddd\t").count(), 2);
}

#[test]
fn reserved_vector_scratch_cannot_be_allocated() {
    let m = keel_ir::parse(
        "func @sum_bad(i64x2) -> i64 {\nblock0(v0: i64x2):\n v1 = vreduce_add v0\n return v1\n}\n",
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::for_level(CpuLevel::V1),
    };
    for scratch_case in 4..=6 {
        let err = keel_codegen::x86_64::compile_with_pipeline(
            &m,
            &keel_codegen::x86_64::REGISTERS,
            &CodegenPipeline {
                selector: &selector,
                allocator: &Controlled(scratch_case),
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("reserved register"), "{err}");
    }
}
