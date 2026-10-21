//! SSE2 unsigned ordering and mask proofs; the exported oracle runs on Linux x86.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
};
use std::{fs, path::PathBuf, process::Command};

#[path = "common/x86_packed.rs"]
mod x86_packed;

const TINY: Pools = Pools {
    caller_saved: &[6, 7],
    callee_saved: &[3],
    float_caller_saved: &[0, 1],
    float_callee_saved: &[],
};
const SPILLED: Pools = Pools {
    float_caller_saved: &[],
    ..TINY
};
const LAYOUTS: [&str; 8] = [
    "lhs",
    "rhs",
    "distinct",
    "same",
    "stacka",
    "stackb",
    "stackresult",
    "stackall",
];

// Force each destructive two-address overlap independently of allocator choices.
struct Controlled;
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "controlled-packed-compare"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let locations = match input.func.name.rsplit('_').next().unwrap() {
            "lhs" | "same" => [Loc::FReg(0), Loc::FReg(1), Loc::FReg(0)],
            "rhs" => [Loc::FReg(0), Loc::FReg(1), Loc::FReg(1)],
            "distinct" => [Loc::FReg(0), Loc::FReg(1), Loc::FReg(2)],
            "stacka" => [Loc::Stack(0), Loc::FReg(1), Loc::FReg(1)],
            "stackb" => [Loc::FReg(0), Loc::Stack(0), Loc::FReg(0)],
            "stackresult" => [Loc::FReg(0), Loc::FReg(1), Loc::Stack(0)],
            "stackall" => [Loc::Stack(0), Loc::Stack(1), Loc::Stack(2)],
            _ => unreachable!(),
        };
        a.spill_slots = 3;
        for (i, loc) in locations.into_iter().enumerate() {
            a.locs[0][i] = Some(loc);
        }
        a
    }
}

fn compile(
    source: &str,
    selection: SelectionTier,
    allocator: &dyn RegisterAllocator,
    pools: &Pools,
    optimize: bool,
) -> (String, Vec<u8>) {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if optimize {
        keel_opt::run_passes_for_target_with_options(
            &mut m,
            keel_opt::DEFAULT_PIPELINE,
            keel_opt::VectorTarget::X86Sse2,
            keel_opt::OptimizeOptions {
                verification: keel_opt::VerificationPolicy::Enabled,
            },
        )
        .unwrap();
    }
    let pipeline = CodegenPipeline {
        selector: &selection,
        allocator,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let p = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    assert_eq!(p.stats().text_bytes, 0);
    let features = keel_asm_ir::x86::Features::default();
    let typed = keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap();
    let parsed = keel_asm::x86::assemble_object_with_features(&text, features).unwrap();
    assert_eq!(typed, parsed, "typed/text ELF mismatch");
    (text, typed)
}

fn command(c: &mut Command) {
    let output = c.output().unwrap();
    assert!(output.status.success(), "{c:?}: {output:?}");
}
fn evidence() -> Option<PathBuf> {
    std::env::var_os("KEEL_BRANCH_MASK_EVIDENCE").map(PathBuf::from)
}
fn export(stem: &str, source: &str, c: &str, asm: &str, object: &[u8]) {
    if let Some(dir) = evidence() {
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{stem}.keel")), source).unwrap();
        fs::write(dir.join(format!("{stem}.c")), c).unwrap();
        fs::write(dir.join(format!("{stem}.s")), asm).unwrap();
        fs::write(dir.join(format!("{stem}.o")), object).unwrap();
    }
}
fn oracle(stem: &str, source: &str, c: &str, asm: &str, object: &[u8]) {
    export(stem, source, c, asm, object);
    let dir = std::env::temp_dir().join(format!("keel-branch-mask-{}-{stem}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("oracle.c");
    fs::write(&input, c).unwrap();
    command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
            ])
            .arg(&input),
    );
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let obj = dir.join("oracle.o");
        fs::write(&obj, object).unwrap();
        let exe = dir.join("oracle");
        command(
            Command::new("clang")
                .args(["-O2", "-msse2", "-mno-avx"])
                .arg(input)
                .arg(obj)
                .arg("-o")
                .arg(&exe),
        );
        command(&mut Command::new(exe));
    }
    fs::remove_dir_all(dir).unwrap();
}

fn controlled_fixture() -> (String, String) {
    let mut source = String::new();
    let mut c = String::from(
        "typedef __UINT64_TYPE__ u64; extern void *memcpy(void*,const void*,__SIZE_TYPE__);\n",
    );
    let mut main = String::from("int main(void) {\n");
    for (shape, ty, lanes) in [
        ("i8x16", "__UINT8_TYPE__", 16),
        ("i16x8", "__UINT16_TYPE__", 8),
        ("i32x4", "__UINT32_TYPE__", 4),
        ("i64x2", "__UINT64_TYPE__", 2),
    ] {
        c += &format!(
            "typedef {ty} U_{shape}; typedef {ty} V_{shape} __attribute__((vector_size(16)));\n"
        );
        for (cc, symbol) in [("ult", "<"), ("ule", "<="), ("ugt", ">"), ("uge", ">=")] {
            for layout in LAYOUTS {
                let name = format!("controlled_{shape}_{cc}_{layout}");
                let rhs = if layout == "same" { "v0" } else { "v1" };
                source += &format!(
                    "func @{name}({shape}, {shape}) -> {shape} {{\nblock0(v0: {shape}, v1: {shape}):\n v2 = vicmp.{cc} v0, {rhs}\n return v2\n}}\n"
                );
                c += &format!("extern V_{shape} {name}(V_{shape},V_{shape});\n");
                let predicate = if layout == "same" {
                    if matches!(cc, "ule" | "uge") {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }
                } else {
                    format!("a[l] {symbol} b[l]")
                };
                main += &format!(
                    "{{ u64 edges[]={{0,1,0x7f,0x80,0xff,0x7fff,0x8000,0xffff,0x7fffffff,0x80000000,0xffffffff,0x100000000,0x17fffffff,0x180000000,0x1ffffffff,0x7fffffff00000000,0x7fffffffffffffff,0x8000000000000000ULL,0x8000000000000001ULL,0xffffffff00000000ULL,0xffffffff7fffffffULL,0xffffffff80000000ULL,0xfffffffffffffffeULL,0xffffffffffffffffULL}};\nfor(unsigned i=0;i<24;i++)for(unsigned j=0;j<24;j++){{ U_{shape} a[{lanes}],b[{lanes}],got[{lanes}];for(unsigned l=0;l<{lanes};l++){{a[l]=(U_{shape})edges[(i+7*l)%24];b[l]=(U_{shape})edges[(j+11*l)%24];}}V_{shape} va,vb,vr;memcpy(&va,a,16);memcpy(&vb,b,16);vr={name}(va,vb);memcpy(got,&vr,16);for(unsigned l=0;l<{lanes};l++)if(got[l]!=(U_{shape})(({predicate}) ? (U_{shape})-1:0))return 1;}}}}\n"
                );
            }
        }
    }
    main += "return 0;}\n";
    (source, c + &main)
}

#[test]
fn unsigned_ordering_is_packed_at_every_width_and_overlap() {
    let (source, c) = controlled_fixture();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        let (asm, object) = compile(
            &source,
            selection,
            &Controlled,
            &keel_codegen::x86_64::REGISTERS,
            false,
        );
        for opcode in ["pcmpgtb", "pcmpgtw", "pcmpgtd", "pslld", "pshufd"] {
            assert!(asm.contains(opcode), "missing {opcode}");
        }
        assert!(!asm.contains("pextr"));
        assert!(!asm.contains("\tcmp\t"), "unsigned compare scalarized");
        oracle(
            &format!("controlled-{selection:?}"),
            &source,
            &c,
            &asm,
            &object,
        );
    }
}

#[test]
fn integer_compare_oracle_preserves_live_inputs_with_both_allocators_and_all_spills() {
    let (source, c) = x86_packed::fixture();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, pools) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("tiny", &TINY),
                ("spilled", &SPILLED),
            ] {
                let (asm, object) = compile(
                    &source,
                    selection,
                    &allocation,
                    pools,
                    matches!(selection, SelectionTier::Full),
                );
                if pool == "spilled" {
                    assert!(asm.contains("xmmword ptr [rbp"));
                }
                oracle(
                    &format!("compare-{selection:?}-{allocation:?}-{pool}"),
                    &source,
                    &c,
                    &asm,
                    &object,
                );
            }
        }
    }
}
