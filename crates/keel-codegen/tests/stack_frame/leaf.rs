//! Stack-free C leaves keep the incoming frame chain intact at the Size goal.
use keel_codegen::{
    CodegenGoal, CodegenPipeline, EmissionOptions, Request, SelectionTier, Target,
    regalloc::AllocationTier,
};

const SOURCE: &str = "func @leaf(i64,i64) -> i64 {
block0(v0:i64,v1:i64):
 v2=iadd v0,v1
 return v2
}
func @private(i64,i64) -> i64 noinline local {
block0(v0:i64,v1:i64):
 v2=xor v0,v1
 return v2
}
func @forward(i64) -> i64 {
block0(v0:i64):
 v1=iconst.i64 7
 v2=call @private(v0,v1)
 return v2
}
func @floating(f64,f64) -> f64 {
block0(v0:f64,v1:f64):
 v2=fadd v0,v1
 return v2
}
func @narrow_helper(i8 sext,i64) -> i64 noinline local {
block0(v0:i8,v1:i64):
 v2=sext.i64 v0
 v3=xor v2,v1
 return v3
}
func @narrow(i64) -> i64 {
block0(v0:i64):
 v1=trunc.i8 v0
 v2=iconst.i64 9
 v3=call @narrow_helper(v1,v2)
 return v3
}";

fn body<'a>(text: &'a str, target: Target, name: &str) -> &'a str {
    let prefix = if target == Target::Aarch64AppleDarwin {
        "_"
    } else {
        ""
    };
    text.split_once(&format!("{prefix}{name}:\n"))
        .unwrap()
        .1
        .split_once(".cfi_endproc")
        .unwrap()
        .0
}

#[test]
fn size_c_leaves_and_changed_argument_tails_preserve_native_results_and_transports() {
    let mut module = keel_ir::parse(SOURCE).unwrap();
    keel_opt::run_passes(&mut module, &["internal-fast-calls", "tail-calls"]).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let pipeline = CodegenPipeline {
            selector: &SelectionTier::Full,
            allocator: &AllocationTier::Full,
        };
        let mut options = EmissionOptions::default();
        options.goal = CodegenGoal::Size;
        let request = Request::new(target, pipeline).emission(options);
        let text = request.text(&module).unwrap();
        for name in ["leaf", "forward", "floating", "narrow"] {
            assert!(
                !body(&text, target, name).contains("x29"),
                "{target:?} {name}: {text}"
            );
        }
        let bytes = keel_asm::assemble_source_for(target, &text).unwrap();
        assert_eq!(
            bytes,
            keel_asm::assemble_program_for(target, &request.program(&module).unwrap()).unwrap()
        );
        let mut chunks = keel_asm_ir::Program::new();
        request
            .program_chunks(&module, &mut |part| chunks.append(part))
            .unwrap();
        assert_eq!(
            bytes,
            keel_asm::assemble_program_for(target, &chunks).unwrap()
        );
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        if target == Target::Aarch64AppleDarwin {
            let dir = std::env::temp_dir().join(format!("keel-size-leaf-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("leaf.o"), &bytes).unwrap();
            std::fs::write(
                dir.join("oracle.c"),
                r#"#include <stdint.h>
extern uint64_t leaf(uint64_t,uint64_t),forward(uint64_t),narrow(uint64_t);
extern double floating(double,double);
int main(void) {
 const uint64_t values[]={0,1,7,UINT64_MAX,UINT64_C(1)<<63};
 for(unsigned i=0;i<5;i++) {
  if(forward(values[i])!=(values[i]^7))return 1;
  if(narrow(values[i])!=((uint64_t)(int64_t)(int8_t)values[i]^9))return 3;
  for(unsigned j=0;j<5;j++)if(leaf(values[i],values[j])!=values[i]+values[j])return 2;
 }
 return floating(1.25,2.5)!=3.75;
}
"#,
            )
            .unwrap();
            assert!(
                std::process::Command::new("clang")
                    .arg(dir.join("oracle.c"))
                    .arg(dir.join("leaf.o"))
                    .arg("-o")
                    .arg(dir.join("run"))
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(
                std::process::Command::new(dir.join("run"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}

#[test]
fn size_leaf_frame_omission_obeys_function_policy_and_frame_dependencies() {
    for (attrs, default, frame) in [
        ("", CodegenGoal::Speed, true),
        ("", CodegenGoal::Size, false),
        ("cold", CodegenGoal::Speed, false),
        ("optimize(\"Os\")", CodegenGoal::Speed, false),
        ("optimize(2)", CodegenGoal::Size, true),
        ("optnone", CodegenGoal::Size, true),
    ] {
        let text = SOURCE.replacen("-> i64 {", &format!("-> i64 {attrs} {{"), 1);
        let module = keel_ir::parse(&text).unwrap();
        let mut options = EmissionOptions::default();
        options.goal = default;
        let text = Request::new(
            Target::Aarch64AppleDarwin,
            CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Full,
            },
        )
        .emission(options)
        .text(&module)
        .unwrap();
        assert_eq!(
            body(&text, Target::Aarch64AppleDarwin, "leaf").contains("x29"),
            frame,
            "{attrs} {default:?}: {text}"
        );
    }
    for source in [
        "decl @effect(i64)->i64
func @blocked(i64)->i64 {
block0(v0:i64):
 v1=call @effect(v0)
 return v1
}",
        "func @blocked()->ptr {
block0:
 v0=intrinsic frame_address()
 return v0
}",
        "func @blocked()->ptr {
 ss0=stack_slot 8,align 8
block0:
 v0=stack_addr ss0
 return v0
}",
        "func @blocked(i64,i64,i64,i64,i64,i64,i64,i64,i64)->i64 {
block0(v0:i64,v1:i64,v2:i64,v3:i64,v4:i64,v5:i64,v6:i64,v7:i64,v8:i64):
 return v8
}",
        "file f0=\"leaf.c\"
func @blocked(i64)->i64 {
block0(v0:i64):
 return v0
}",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let mut options = EmissionOptions::default();
        options.goal = CodegenGoal::Size;
        let text = Request::new(
            Target::Aarch64AppleDarwin,
            CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Full,
            },
        )
        .emission(options)
        .text(&module)
        .unwrap();
        assert!(
            body(&text, Target::Aarch64AppleDarwin, "blocked").contains("x29"),
            "{source}: {text}"
        );
    }
}
