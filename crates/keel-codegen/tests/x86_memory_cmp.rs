//! Adjacent ordinary loads selected into CMP, with allocation-visible addresses.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    select::{Selection, Target as SelectTarget},
};

const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn source(condition: &str) -> String {
    format!(
        "func @f(ptr, i64) -> i64 {{
block0(v0: ptr, v1: i64):
 v2 = load.i64 v0
 v3 = icmp.{condition} v1, v2
 brif v3, block1(v1), block2(v1)
block1(v4: i64):
 v5 = iconst.i64 17
 v6 = iadd v4, v5
 return v6
block2(v7: i64):
 v8 = iconst.i64 91
 v9 = iadd v7, v8
 return v9
}}"
    )
}

fn module(source: &str) -> keel_ir::Module {
    let m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}

fn memory_compares(text: &str) -> usize {
    text.lines()
        .filter(|line| line.trim_start().starts_with("cmp\t") && line.contains("qword ptr"))
        .count()
}

fn compile(
    m: &keel_ir::Module,
    tier: SelectionTier,
    alloc: AllocationTier,
    pools: &Pools,
) -> String {
    let pipeline = CodegenPipeline {
        selector: &tier,
        allocator: &alloc,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    assert_eq!(
        keel_asm::x86::assemble_program_object(&program).unwrap(),
        keel_asm::x86::assemble_object(&text).unwrap(),
        "{text}"
    );
    text
}

#[test]
fn exact_width_flags_conditions_edges_and_spilled_addresses() {
    for cc in [
        "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
    ] {
        for tier in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&SPILLED, &keel_codegen::x86_64::REGISTERS] {
                    let m = module(&source(cc));
                    let text = compile(&m, tier, alloc, pools);
                    assert_eq!(
                        memory_compares(&text),
                        usize::from(tier != SelectionTier::Baseline),
                        "{text}"
                    );
                }
            }
        }
    }
    // `notrap` is not required: the original exact eight-byte read still occurs.
    for flags in ["", "align 8 ", "notrap ", "align 8 notrap "] {
        let m = module(&source("ult").replace("load.i64 v0", &format!("load.i64 {flags}v0")));
        assert_eq!(
            memory_compares(&compile(
                &m,
                SelectionTier::Full,
                AllocationTier::Full,
                &SPILLED
            )),
            1
        );
    }
}

#[test]
fn existing_address_folding_and_unfused_boolean_keep_live_leaves() {
    let src = source("ult").replace(
        "v2 = load.i64 v0",
        "v10 = iconst.i64 216\n v11 = ptradd v0, v10\n v2 = load.i64 v11",
    );
    let m = module(&src);
    let f = &m.functions[0];
    let b = f.blocks().next().unwrap();
    let insts = f.block_insts(b);
    let plan =
        Selection::compute_with_options(f, SelectTarget::X86, keel_codegen::SelectionOptions::ALL);
    assert!(plan.covered(insts[1]), "ptradd remains covered");
    assert!(plan.covered(insts[2]), "load is covered");
    assert_eq!(
        plan.operands(f, insts[3]),
        vec![f.block_params(b)[1], f.block_params(b)[0]]
    );
    let text = compile(&m, SelectionTier::Full, AllocationTier::Full, &SPILLED);
    assert_eq!(memory_compares(&text), 1);
    assert!(
        text.lines()
            .any(|l| l.contains("cmp\t") && l.contains("216")),
        "{text}"
    );

    let m = module(
        "func @f(ptr, i64) -> i64 {\nblock0(v0: ptr, v1: i64):\n v2 = load.i64 v0\n v3 = icmp.ult v1, v2\n v4 = uext.i64 v3\n return v4\n}",
    );
    let text = compile(&m, SelectionTier::Full, AllocationTier::Full, &SPILLED);
    assert_eq!(memory_compares(&text), 1, "{text}");
    assert!(text.contains("setb"), "{text}");
}

#[test]
fn shared_volatile_atomic_nonadjacent_left_and_debug_loads_stay_materialized() {
    let src = source("ult");
    let negatives = [
        src.replace("load.i64 v0", "load.i64 volatile v0"),
        src.replace("load.i64 v0", "atomic_load.i64 relaxed v0"),
        src.replace("icmp.ult v1, v2", "icmp.ult v2, v1"),
        src.replace("v3 = icmp", "v10 = iconst.i64 99\n v3 = icmp"),
        src.replace("v3 = icmp", "store v1, v0\n v3 = icmp"),
        src.replace("block1(v1), block2(v1)", "block1(v2), block2(v1)"),
        src.replace("v3 = icmp", "jump block3\nblock3:\n v3 = icmp"),
        format!(
            "file f0 = \"memory.c\"\n{}",
            src.replace("load.i64 v0", "load.i64 v0 !f0:2:1")
        ),
        format!(
            "file f0 = \"memory.c\"\n{}",
            src.replace("icmp.ult v1, v2", "icmp.ult v1, v2 !f0:3:1")
        ),
    ];
    for negative in negatives {
        let m = module(&negative);
        let text = compile(&m, SelectionTier::Full, AllocationTier::Full, &SPILLED);
        assert_eq!(memory_compares(&text), 0, "{negative}\n{text}");
    }
    for ty in ["i8", "i16", "i32", "i128"] {
        let src = format!(
            "func @f(ptr, {ty}) -> i64 {{\nblock0(v0: ptr, v1: {ty}):\n v2 = load.{ty} v0\n v3 = icmp.ult v1, v2\n v4 = uext.i64 v3\n return v4\n}}"
        );
        let m = module(&src);
        let f = &m.functions[0];
        let b = f.blocks().next().unwrap();
        let plan = Selection::compute_with_options(
            f,
            SelectTarget::X86,
            keel_codegen::SelectionOptions::ALL,
        );
        assert!(!plan.covered(f.block_insts(b)[0]), "{src}");
        // i128's existing lowering itself compares a half against a spill.
        // Check the original load remains, rather than counting that unrelated CMP.
        compile(&m, SelectionTier::Full, AllocationTier::Full, &SPILLED);
    }
}

#[test]
fn aliases_preserve_address_uses_and_never_cover_a_dead_definition() {
    let m = module(&source("ult"));
    let mut f = m.functions[0].clone();
    let b = f.blocks().next().unwrap();
    let insts = f.block_insts(b).to_vec();
    let loaded = f.inst_results(insts[0])[0];
    let lhs = f.block_params(b)[1];
    f.replace_all_uses(loaded, lhs); // Retained load is a dead Alias definition.
    keel_ir::verify(&f).unwrap();
    let plan =
        Selection::compute_with_options(&f, SelectTarget::X86, keel_codegen::SelectionOptions::ALL);
    assert!(!plan.covered(insts[0]));
}

#[test]
fn resolved_operand_aliases_and_aliased_comparison_definition() {
    let src = source("ult")
        .replace("@f(ptr, i64)", "@f(ptr, i64, ptr)")
        .replace("v1: i64):", "v1: i64, v10: ptr):");
    let mut m = module(&src);
    let f = &mut m.functions[0];
    let b = f.blocks().next().unwrap();
    let params = f.block_params(b).to_vec();
    f.replace_all_uses(params[0], params[2]);
    keel_ir::verify_module(&m).unwrap();
    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
        assert_eq!(
            memory_compares(&compile(&m, SelectionTier::Full, alloc, &SPILLED)),
            1
        );
    }
    // The raw RHS names an Alias but resolves to the single live load result.
    let src = source("ult")
        .replace("v2 = load", "v10 = iconst.i64 7\n v2 = load")
        .replace("icmp.ult v1, v2", "icmp.ult v1, v10");
    let mut m = module(&src);
    let f = &mut m.functions[0];
    let b = f.blocks().next().unwrap();
    let insts = f.block_insts(b).to_vec();
    f.replace_all_uses(f.inst_results(insts[0])[0], f.inst_results(insts[1])[0]);
    keel_ir::verify_module(&m).unwrap();
    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
        assert_eq!(
            memory_compares(&compile(&m, SelectionTier::Full, alloc, &SPILLED)),
            1
        );
    }
    let src = source("ult")
        .replace("@f(ptr, i64)", "@f(ptr, i64, i8)")
        .replace("v1: i64):", "v1: i64, v10: i8):");
    let mut m = module(&src);
    let f = &mut m.functions[0];
    let b = f.blocks().next().unwrap();
    let insts = f.block_insts(b).to_vec();
    f.replace_all_uses(f.inst_results(insts[1])[0], f.block_params(b)[2]);
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    let plan =
        Selection::compute_with_options(f, SelectTarget::X86, keel_codegen::SelectionOptions::ALL);
    assert!(
        !plan.covered(insts[0]),
        "retained aliased compare cannot own a new cover"
    );
    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
        compile(&m, SelectionTier::Full, alloc, &SPILLED);
    }
}

#[test]
fn whole_function_debug_metadata_and_covered_condition_chain_fall_back() {
    let src = source("ult");
    for src in [
        format!(
            "file f0 = \"memory.c\"\n{}",
            src.replace("{\nblock0", "{\n dloc f0:1:1\nblock0")
        ),
        format!(
            "file f0 = \"memory.c\"\ndtype dt0 = base \"int\" signed 4\n{}",
            src.replace(
                "{\nblock0",
                "{\n ss0 = stack_slot 4, align 4\n dvar \"x\" dt0 ss0\nblock0"
            )
        ),
        format!(
            "file f0 = \"memory.c\"\n{}",
            src.replace("iconst.i64 17", "iconst.i64 17 !f0:9:1")
        ),
        src.replace(
            "brif v3",
            "v10 = uext.i64 v3\n v11 = iconst.i64 0\n v12 = icmp.ne v10, v11\n brif v12",
        ),
    ] {
        let m = module(&src);
        let f = &m.functions[0];
        let b = f.blocks().next().unwrap();
        let plan = Selection::compute_with_options(
            f,
            SelectTarget::X86,
            keel_codegen::SelectionOptions::ALL,
        );
        assert!(!plan.covered(f.block_insts(b)[0]), "{src}");
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            compile(&m, SelectionTier::Full, alloc, &SPILLED);
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_unsigned_signed_and_boundaries_match_independent_clang_oracle() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("x86-memory-cmp");
    fs::create_dir_all(&dir).unwrap();
    let mut ir = String::new();
    let mut oracle = String::from("#include <stdint.h>\n");
    let mut driver = String::from(
        "#include <stdint.h>\n#include <assert.h>\n#include <unistd.h>\n#include <sys/mman.h>\n#include <sys/wait.h>\n#include <signal.h>\n",
    );
    let mut checks = String::new();
    for (index, (cc, op, signed)) in [
        ("eq", "==", false),
        ("ne", "!=", false),
        ("slt", "<", true),
        ("sle", "<=", true),
        ("sgt", ">", true),
        ("sge", ">=", true),
        ("ult", "<", false),
        ("ule", "<=", false),
        ("ugt", ">", false),
        ("uge", ">=", false),
    ]
    .into_iter()
    .enumerate()
    {
        ir.push_str(&source(cc).replace("@f(", &format!("@f{index}(")));
        oracle.push_str(&format!("uint64_t o{index}(const uint64_t*p,uint64_t x){{return x+(({})x {op} ({})*p?17:91);}}\n", if signed {"int64_t"}else{"uint64_t"}, if signed {"int64_t"}else{"uint64_t"}));
        driver.push_str(&format!("extern uint64_t f{index}(const void*,uint64_t);extern uint64_t o{index}(const uint64_t*,uint64_t);\n"));
        checks.push_str(&format!("assert(f{index}(p,x)==o{index}(p,x));"));
    }
    driver.push_str(&format!("int main(void){{uint64_t a[]={{0,1,2,0x7fffffffffffffffULL,0x8000000000000000ULL,UINT64_MAX}};for(unsigned i=0;i<6;i++)for(unsigned j=0;j<6;j++){{uint64_t*p=&a[i],x=a[j];{checks}}}long n=sysconf(_SC_PAGESIZE);assert(n>0);unsigned char*m=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);assert(m!=MAP_FAILED);uint64_t*p=(uint64_t*)(m+n-8);*p=UINT64_MAX;assert(mprotect(m+n,n,PROT_NONE)==0);for(unsigned j=0;j<6;j++){{uint64_t x=a[j];{checks}}}pid_t c=fork();assert(c>=0);if(!c){{(void)f0(m+n-7,0);_exit(0);}}int s;assert(waitpid(c,&s,0)==c);assert(WIFSIGNALED(s));assert(WTERMSIG(s)==SIGSEGV||WTERMSIG(s)==SIGBUS);assert(munmap(m,2*n)==0);return 0;}}\n"));
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    fs::write(dir.join("driver.c"), driver).unwrap();
    assert!(
        Command::new("clang")
            .args(["-O0", "-fno-lto", "-c"])
            .arg(dir.join("oracle.c"))
            .arg("-o")
            .arg(dir.join("oracle.o"))
            .status()
            .unwrap()
            .success()
    );
    let m = module(&ir);
    for (index, (tier, alloc, pools)) in [
        (
            SelectionTier::Baseline,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
        ),
        (
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
        ),
        (
            SelectionTier::Full,
            AllocationTier::Fast,
            &keel_codegen::x86_64::REGISTERS,
        ),
        (SelectionTier::Full, AllocationTier::Fast, &SPILLED),
        (SelectionTier::Full, AllocationTier::Full, &SPILLED),
    ]
    .into_iter()
    .enumerate()
    {
        let text = compile(&m, tier, alloc, pools);
        fs::write(dir.join("test.s"), text).unwrap();
        let binary = dir.join(format!("test-{index}"));
        assert!(
            Command::new("clang")
                .arg(dir.join("test.s"))
                .arg(dir.join("driver.c"))
                .arg(dir.join("oracle.o"))
                .arg("-o")
                .arg(&binary)
                .status()
                .unwrap()
                .success()
        );
        assert!(Command::new(binary).status().unwrap().success());
    }
}
