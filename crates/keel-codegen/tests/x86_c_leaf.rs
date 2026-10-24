//! C leaves may omit their frame only when the stack and its observers are absent.
use keel_asm_ir::{
    Cfi, Directive, Item,
    x86::{CpuLevel, Features},
};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
    x86_features::FeatureSelector,
};
use keel_ir::EntityRef;
use object::{Object, ObjectSection};

const LEAF: &str = "func @leaf(i64) -> i64 cc(c) {
block0(v0: i64):
 v1 = iconst.i64 42
 v2 = iadd v0, v1
 return v2
}";
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn compile(
    src: &str,
    tier: SelectionTier,
    alloc: &dyn RegisterAllocator,
    pools: &Pools,
    wide: bool,
) -> (String, Vec<u8>, Vec<Cfi>) {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &tier,
        features: Features::for_level(if wide { CpuLevel::V3 } else { CpuLevel::V1 }),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: alloc,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let bytes =
        keel_asm::x86::assemble_program_object_with_features(&program, selector.features).unwrap();
    assert_eq!(
        bytes,
        keel_asm::x86::assemble_object_with_features(&text, selector.features).unwrap()
    );
    let cfi = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Directive(Directive::Cfi(cfi)) => Some(cfi.clone()),
            _ => None,
        })
        .collect();
    (text, bytes, cfi)
}

fn assert_leaf(text: &str, bytes: &[u8], cfi: &[Cfi]) {
    assert!(
        !text.contains("\tpush\t") && !text.contains("\tpop\t"),
        "{text}"
    );
    assert!(
        !text.contains("rbp") && !text.contains("\tsub\trsp"),
        "{text}"
    );
    // These are the entire function's CFI rules: CFA = rsp + 8 at every PC.
    assert_eq!(
        cfi,
        &[
            Cfi::Start,
            Cfi::DefCfa {
                register: 7,
                offset: 8
            },
            Cfi::End
        ]
    );
    let file = object::File::parse(bytes).unwrap();
    let eh = file.section_by_name(".eh_frame").unwrap().data().unwrap();
    // The CIE defines CFA = rsp + 8 and RIP = *(CFA - 8).
    let cie_end = 4 + u32::from_le_bytes(eh[..4].try_into().unwrap()) as usize;
    assert!(eh[..cie_end].windows(5).any(|w| w == [0x0c, 7, 8, 0x90, 1]));
    // FDE: length, CIE pointer, start, range, zero augmentation length,
    // then DW_CFA_def_cfa rsp, 8 and padding. No saved-rbp rule.
    let fde = &eh[cie_end..];
    let len = u32::from_le_bytes(fde[..4].try_into().unwrap()) as usize;
    assert_eq!(&fde[16..20], &[0, 0x0c, 7, 8]);
    assert!(fde[20..len + 4].iter().all(|&b| b == 0));
}

#[test]
fn scalar_and_simd_c_leaves_keep_rsp_based_unwind_rules() {
    let simd = "func @leaf(ptr, ptr, ptr) cc(c) {
block0(v0: ptr, v1: ptr, v2: ptr):
 v3 = load.i32x4 v0
 v4 = load.i32x4 v1
 v5 = viadd v3, v4
 store v5, v2
 return
}";
    for tier in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for src in [LEAF, simd] {
                let (text, bytes, cfi) =
                    compile(src, tier, &alloc, &keel_codegen::x86_64::REGISTERS, false);
                assert_leaf(&text, &bytes, &cfi);
                if src == simd {
                    assert!(
                        !text.contains("xmm14"),
                        "GPR entry copies need no SIMD temp: {text}"
                    );
                }
            }
        }
    }
}

#[test]
fn stack_users_observers_and_hidden_calls_retain_frames() {
    let inserted = |inst: &str| LEAF.replace(" v1 = iconst", &format!(" {inst}\n v1 = iconst"));
    let mut sources = vec![
        inserted("v8 = intrinsic frame_address()"),
        inserted("v8 = intrinsic stack_save()"),
        inserted("v8 = intrinsic stack_alloc(v0)"),
        format!("file f0 = \"leaf.c\"\n{LEAF}"),
        format!("decl @outside()\n{}", inserted("call @outside()")),
        "func @f(i64,i64,i64,i64,i64,i64,i64) -> i64 cc(c) {\nblock0(v0:i64,v1:i64,v2:i64,v3:i64,v4:i64,v5:i64,v6:i64):\n return v6\n}".into(),
        "func @f(i64, ...) -> i64 cc(c) {\nblock0(v0:i64):\n return v0\n}".into(),
        "func @f() -> ptr cc(c) {\n ss0 = stack_slot 8, align 8\nblock0:\n v0 = stack_addr ss0\n return v0\n}".into(),
        "data @tls = thread_local align 8 { i64 5 }\nfunc @f() -> ptr cc(c) {\nblock0:\n v0 = symbol_addr @tls\n return v0\n}".into(),
        "func @f(f64,f64,f64) -> f64 cc(c) {\nblock0(v0:f64,v1:f64,v2:f64):\n v3 = fma v0,v1,v2\n return v3\n}".into(),
        "func @f(i128,i128) -> i128 cc(c) {\nblock0(v0:i128,v1:i128):\n v2 = udiv v0,v1\n return v2\n}".into(),
    ];
    // Stack storage is required even if the stack address never escapes.
    sources.push("func @f(ptr) cc(c) {\n ss0 = stack_slot 16, align 32\nblock0(v0:ptr):\n v1 = stack_addr ss0\n v2 = load.i64 v0\n store v2,v1\n return\n}".into());
    for src in sources {
        for tier in [SelectionTier::Baseline, SelectionTier::Full] {
            let (text, _, cfi) = compile(
                &src,
                tier,
                &AllocationTier::Full,
                &keel_codegen::x86_64::REGISTERS,
                false,
            );
            assert!(text.contains("\tpush\trbp"), "{src}\n{text}");
            assert!(cfi.contains(&Cfi::DefCfaRegister(6)), "{text}");
        }
    }
    let personality = keel_ir::parse(&format!(
        "decl @p()\n{}",
        LEAF.replace("cc(c)", "cc(c) personality @p")
    ))
    .unwrap();
    let error =
        keel_codegen::compile(&personality, keel_codegen::Target::X86_64LinuxGnu).unwrap_err();
    assert!(
        error.to_string().contains("exceptions are not supported"),
        "{error}"
    );
    for pools in [
        EMPTY,
        Pools {
            caller_saved: &[],
            callee_saved: &[3],
            ..EMPTY
        },
    ] {
        let (text, _, _) = compile(
            LEAF,
            SelectionTier::Full,
            &AllocationTier::Full,
            &pools,
            false,
        );
        assert!(text.contains("\tpush\trbp"), "{text}");
    }
}

// Force entry and CFG-edge cycles independently of allocator preferences.
struct Cycle {
    spilled: bool,
    wide: bool,
}
impl RegisterAllocator for Cycle {
    fn name(&self) -> &'static str {
        "leaf-copy-cycle"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let step = if self.wide { 4 } else { 2 };
        for &block in &a.order {
            let params = input.func.block_params(block);
            for (i, &value) in params.iter().enumerate().take(2) {
                a.locs[0][value.index()] = Some(if self.spilled {
                    Loc::Stack(i as u32 * step)
                } else {
                    Loc::FReg(1 - i as u8)
                });
            }
        }
        if self.spilled {
            a.spill_slots = 2 * step;
        }
        a
    }
}

#[test]
fn simd_register_and_spill_cycles_preserve_every_lane() {
    for wide in [false, true] {
        let ty = if wide { "i32x8" } else { "i32x4" };
        let src = format!(
            "func @cycle({ty},{ty},ptr,ptr) cc(c) {{
block0(v0:{ty},v1:{ty},v2:ptr,v3:ptr):
 jump block1(v1,v0)
block1(v4:{ty},v5:{ty}):
 store v4,v2
 store v5,v3
 return
}}"
        );
        for spilled in [false, true] {
            let (text, bytes, cfi) = compile(
                &src,
                SelectionTier::Full,
                &Cycle { spilled, wide },
                &keel_codegen::x86_64::REGISTERS,
                wide,
            );
            assert!(
                text.contains(if wide { "ymm14" } else { "xmm14" }),
                "{text}"
            );
            if !wide && !spilled {
                assert_leaf(&text, &bytes, &cfi);
            } else {
                assert!(text.contains("\tpush\trbp"), "{text}");
            }
            #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
            if !wide || std::is_x86_feature_detected!("avx2") {
                use std::{fs, process::Command};
                let dir = std::env::temp_dir()
                    .join(format!("keel-leaf-{}-{wide}-{spilled}", std::process::id()));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("cycle.o"), bytes).unwrap();
                fs::write(
                    dir.join("main.c"),
                    format!(
                        "
 typedef unsigned V __attribute__((vector_size({})));
 extern void cycle(V,V,V*,V*);
 int main(void) {{
   V a,b,x,y;
   for(unsigned i=0;i<sizeof(V)/4;i++) {{ a[i]=0x12340000+i; b[i]=0x98760000+i; }}
   cycle(a,b,&x,&y);
   for(unsigned i=0;i<sizeof(V)/4;i++) if(x[i]!=b[i] || y[i]!=a[i]) return 1;
   return 0;
 }}",
                        if wide { 32 } else { 16 }
                    ),
                )
                .unwrap();
                let output = Command::new("clang")
                    .arg(if wide { "-mavx2" } else { "-msse2" })
                    .arg(dir.join("main.c"))
                    .arg(dir.join("cycle.o"))
                    .arg("-o")
                    .arg(dir.join("run"))
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{output:?}");
                assert!(
                    Command::new(dir.join("run")).status().unwrap().success(),
                    "{text}"
                );
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}
