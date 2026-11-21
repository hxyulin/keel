#[path = "kernel_code_model/unwind_flags.rs"]
mod unwind_flags;

use object::{Object, ObjectSection};
use std::{fs, process::Command};

#[test]
fn frame_size_warning_reports_layout_once_per_function() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("frame-size-warning");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("frames.c"),
        r#"
        extern void use(void *);
        void large(void) { char buffer[3200]; use(buffer); }
        void second(void) { char buffer[4096]; use(buffer); }
        int small(int n) { return n + 1; }
        void dynamic(int n) { use(__builtin_alloca(n)); }
    "#,
    )
    .unwrap();
    for target in ["x86_64-linux-gnu", "aarch64-linux-gnu"] {
        for mode in [
            &["-Os", "--codegen-threads", "1"][..],
            &["-Os", "--codegen-threads", "2"],
            &["--mode", "speed"],
        ] {
            let result = Command::new(env!("CARGO_BIN_EXE_keel"))
                .current_dir(&dir)
                .args([
                    "cc",
                    "--target",
                    target,
                    "-Wframe-larger-than=2048",
                    "-save-temps=obj",
                    "-c",
                    "frames.c",
                    "-o",
                    "frames.o",
                ])
                .args(mode)
                .output()
                .unwrap();
            let stderr = String::from_utf8(result.stderr).unwrap();
            assert!(result.status.success(), "{target} {mode:?}: {stderr}");
            assert_eq!(
                stderr.matches("[-Wframe-larger-than]").count(),
                2,
                "{stderr}"
            );
            assert!(
                stderr.find("large:").unwrap() < stderr.find("second:").unwrap(),
                "{stderr}"
            );
        }
    }
}

#[test]
fn kernel_function_flags_survive_ir_round_trip() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("kernel-function-flags");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("entry.c"),
        "__attribute__((noinline)) static long add(long x) { return x + 1; }\n\
         long entry(long x) { return add(x); }",
    )
    .unwrap();
    let run = |args: &[&str]| {
        let result = Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    };
    let ir = run(&[
        "cc",
        "--target=x86_64-linux-gnu",
        "-p",
        "tail-calls",
        "-fno-optimize-sibling-calls",
        "-fno-omit-frame-pointer",
        "-fconserve-stack",
        "-fmin-function-alignment=16",
        "--emit-ir",
        "entry.c",
    ]);
    assert!(ir.contains("frame_pointer no_sibling_calls"), "{ir}");
    assert!(ir.contains("conserve_stack"), "{ir}");
    assert!(
        ir.contains("call @add") && !ir.contains("return_call"),
        "{ir}"
    );
    fs::write(dir.join("entry.keel"), ir).unwrap();
    let asm = run(&[
        "compile",
        "--target=x86_64-linux-gnu",
        "--mode",
        "speed",
        "entry.keel",
    ]);
    assert!(asm.contains("\tpush\trbp"), "{asm}");
    assert!(asm.contains("\t.p2align\t4"), "{asm}");
}

#[test]
fn sse_disabled_variadic_calls_and_prologues_use_integer_registers() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("kernel-variadic");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("varargs.c"),
        r#"
__attribute__((noinline)) int sum(int x, ...) {
    __builtin_va_list list;
    __builtin_va_start(list, x);
    x += __builtin_va_arg(list, int);
    __builtin_va_end(list);
    return x;
}
int entry(int x) { return sum(x, 9) + 1; }
"#,
    )
    .unwrap();
    let compile = |flags: &[&str]| {
        let result = Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args(["cc", "--target=x86_64-linux-gnu", "-Os"])
            .args(flags)
            .arg("varargs.c")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    };
    let baseline = compile(&[]);
    assert!(baseline.contains("xmm"), "{baseline}");
    let integer = compile(&["-mno-sse", "-mno-sse2", "-mskip-rax-setup"]);
    assert!(!integer.contains("xmm"), "{integer}");
    assert!(integer.contains("176"), "{integer}");
    let entry = integer.split_once("\n\"entry\":").unwrap().1;
    assert!(!entry.contains("\txor\teax, eax"), "{entry}");
}

#[test]
fn kernel_auto_init_respects_opt_out_and_preserves_potential_faults() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("kernel-auto-init");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("local.c"),
        r#"
    extern void use(void *);
    int f(int *p) {
        volatile int zero;
        volatile int untouched __attribute__((__uninitialized__));
        use((void *)&zero);
        use((void *)&untouched);
        return *p;
    }
    "#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .args([
            "cc",
            "--target=x86_64-linux-gnu",
            "--emit-ir",
            "-ftrivial-auto-var-init=zero",
            "-fno-delete-null-pointer-checks",
            "local.c",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ir = String::from_utf8(out.stdout).unwrap();
    assert!(ir.contains("preserve_memory_faults"), "{ir}");
    assert_eq!(ir.matches("store volatile").count(), 1, "{ir}");
    assert!(!ir.contains("trap_ub"), "{ir}");
}

#[test]
fn kernel_memory_expansion_keeps_small_copies_in_general_registers() {
    let dir = std::env::temp_dir().join(format!("keel-kernel-copy-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("copy.c");
    fs::write(
        &input,
        "void copy(void *dst, const void *src) { __builtin_memcpy(dst, src, 16); }",
    )
    .unwrap();
    for level in ["-O1", "-Os"] {
        for model in ["kernel", "pic"] {
            let result = Command::new(env!("CARGO_BIN_EXE_keel"))
                .args([
                    "cc",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--code-model",
                    model,
                    level,
                ])
                .arg(&input)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{level} {model}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let asm = String::from_utf8(result.stdout).unwrap();
            assert!(!asm.contains("memcpy"), "{level} {model}: {asm}");
            assert_eq!(
                asm.contains("xmm"),
                model == "pic",
                "{level} {model}: {asm}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn size_codegen_obeys_frame_abi_and_symbol_ranges() {
    use std::fmt::Write as _;

    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("kernel-size");
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("size.c");
    let mut source = String::from(
        r#"
        extern char cache[192];
        extern char *pointer;
        extern volatile int stamp;
        extern long callee(long);
        extern void clobber(void);

        void store_address(void) { pointer = cache; }
        int read_stamp(void) { return stamp; }
        void copy_in(const void *src) { __builtin_memmove(cache, src, 96); }
        long pass_result(long x) { return callee(x); }
        long keep_result(long x) {
            long result = callee(x);
            clobber();
            return result;
        }
        int fixed_frame(int x) {
            volatile int words[96];
            int from_asm;
            words[0] = x;
            words[31] = x + 3;
            words[95] = x + 7;
            __asm__ volatile ("pushq %%r10\n\tmovl %1, %0\n\tpopq %%r10"
                              : "=r"(from_asm) : "m"(words[0]) : "memory");
            return from_asm + words[31] + words[95];
        }
        extern int route_0(int), route_1(int), route_2(int), route_3(int);
        __attribute__((cold)) int dispatch(unsigned key, int input) {
            switch (key) {
    "#,
    );
    for target in 0..4 {
        for key in (41..=104).filter(|key| key % 7 != 0 && key % 4 == target) {
            writeln!(source, "case {key}:").unwrap();
        }
        writeln!(source, "return route_{target}(input);").unwrap();
    }
    source.push_str("default: return input + 9; } }");
    fs::write(&input, source).unwrap();

    let compile = |model: &str, extra: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_keel"));
        command.args([
            "cc",
            "--target",
            "x86_64-linux-gnu",
            "--code-model",
            model,
            "--link-intent",
            "executable",
            "-Os",
            "-fno-omit-frame-pointer",
            "-fno-optimize-sibling-calls",
        ]);
        if model == "kernel" {
            command.args(["-mno-sse", "-mstack-alignment=8"]);
        }
        let output = command.args(extra).arg(&input).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    for (model, address, relocation_type) in [
        ("pic", "\"cache\"@GOTPCREL", object::elf::R_X86_64_GOTPCREL),
        (
            "small",
            "mov\tedi, offset \"cache\"",
            object::elf::R_X86_64_32,
        ),
        (
            "kernel",
            "mov\trdi, offset \"cache\"",
            object::elf::R_X86_64_32S,
        ),
    ] {
        let asm = String::from_utf8(compile(model, &[])).unwrap();
        let body = |name: &str| {
            asm.split_once(&format!("\"{name}\":"))
                .unwrap()
                .1
                .split_once(&format!(".size \"{name}\""))
                .unwrap()
                .0
        };
        let frame = body("fixed_frame");
        assert!(
            frame.contains("[rsp +") && frame.contains("[rbp +"),
            "{model}: {frame}"
        );
        let store = body("store_address");
        assert_eq!(
            store.contains("], offset \"cache\""),
            model == "kernel",
            "{store}"
        );
        let load = body("read_stamp");
        assert_eq!(
            load.contains("dword ptr [\"stamp\"]"),
            model == "kernel",
            "{load}"
        );
        let copy = body("copy_in");
        let move_source = copy.find("mov\trsi, rdi").unwrap();
        let move_symbol = copy.find(address).unwrap();
        assert!(move_source < move_symbol, "{copy}");
        assert!(copy.contains("mov\tedx, 96"), "{copy}");
        let after_call = body("pass_result").split_once("@PLT").unwrap().1;
        assert!(!after_call.contains("rax"), "{after_call}");
        let after_clobber = body("keep_result").rsplit_once("@PLT").unwrap().1;
        assert!(after_clobber.contains("mov\trax,"), "{after_clobber}");
        assert!(body("dispatch").contains("movzx\teax, byte ptr"));

        let file_path = dir.join("size.o");
        compile(model, &["--emit-object", "-o", file_path.to_str().unwrap()]);
        let object = fs::read(file_path).unwrap();
        let from_asm = keel_asm::x86::assemble_object(&asm).unwrap();
        let structured = object::File::parse(&*object).unwrap();
        let textual = object::File::parse(&*from_asm).unwrap();
        let text = structured.section_by_name(".text").unwrap();
        assert_eq!(
            text.data().unwrap(),
            textual.section_by_name(".text").unwrap().data().unwrap()
        );
        assert!(
            text.relocations().any(|(_, relocation)| {
                relocation.flags()
                    == (object::RelocationFlags::Elf {
                        r_type: relocation_type,
                    })
            }),
            "{model}"
        );
    }
}

#[test]
fn c_and_ir_cli_choose_kernel_addresses_and_reject_incompatible_targets() {
    let dir = std::env::temp_dir().join(format!("keel-kernel-model-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("address.c"), "extern int external; int defined; void *address(void){return &external;} void *local(void){return &defined;}").unwrap();
    for command in ["cc", "compile"] {
        let input = if command == "cc" {
            "address.c"
        } else {
            "address.keel"
        };
        if command == "compile" {
            let ir = Command::new(env!("CARGO_BIN_EXE_keel"))
                .current_dir(&dir)
                .args([
                    "cc",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--emit-ir",
                    "address.c",
                ])
                .output()
                .unwrap();
            assert!(
                ir.status.success(),
                "{}",
                String::from_utf8_lossy(&ir.stderr)
            );
            fs::write(dir.join(input), ir.stdout).unwrap();
        }
        for model in ["pic", "small", "kernel"] {
            let output = format!("{command}-{model}.o");
            let result = Command::new(env!("CARGO_BIN_EXE_keel"))
                .current_dir(&dir)
                .args([
                    command,
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--code-model",
                    model,
                    "--emit-object",
                    input,
                    "-o",
                    &output,
                ])
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let bytes = fs::read(dir.join(output)).unwrap();
            let file = object::File::parse(&*bytes).unwrap();
            let kinds: Vec<_> = file
                .section_by_name(".text")
                .unwrap()
                .relocations()
                .map(|(_, r)| r.kind())
                .collect();
            assert_eq!(
                kinds
                    .iter()
                    .filter(|&&kind| kind == object::RelocationKind::Absolute)
                    .count(),
                if model == "pic" { 0 } else { 2 }
            );
        }
    }
    for flags in [
        vec![
            "--target",
            "aarch64-unknown-linux-gnu",
            "--code-model",
            "kernel",
        ],
        vec![
            "--target",
            "x86_64-unknown-linux-gnu",
            "--code-model",
            "kernel",
            "--link-intent",
            "shared",
        ],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .arg("cc")
            .args(flags)
            .arg("address.c")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("code model"));
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn static_code_models_link_only_when_absolute_address_relocations_fit() {
    use keel_codegen::{
        CodegenPipeline, EmissionOptions, Request, SelectionTier, Target, regalloc::AllocationTier,
        x86_64::CodeModel,
    };
    use keel_link::{Input, elf};
    let module = keel_ir::parse("data @value = align 8 { i64 7 }\nfunc @_start() -> ptr {\nblock0:\n v0 = symbol_addr @value\n return v0\n}\n").unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    for (model, relocation) in [(CodeModel::Small, 10), (CodeModel::Kernel, 11)] {
        let mut emission = EmissionOptions::default();
        emission.x86_code_model = Some(model);
        let source = Request::new(Target::X86_64LinuxGnu, pipeline)
            .emission(emission)
            .text(&module)
            .unwrap();
        let bytes = keel_asm::x86::assemble_object(&source).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        assert!(
            file.section_by_name(".text")
                .unwrap()
                .relocations()
                .any(|(_, r)| r.flags() == object::RelocationFlags::Elf { r_type: relocation }),
            "{source}"
        );
        for address in [0x1000000u64, 0xffffffff81000000, 0x100000000] {
            let script = format!(
                "ENTRY(physical_entry) PHDRS {{ text PT_LOAD FLAGS(7); }} SECTIONS {{ . = {address}; .text : AT(0x1000000) {{ *(.text) }} :text physical_entry = 0x1000000; .data : AT(ADDR(.data) - {address} + 0x1000000) {{ *(.data) *(.bss) }} :text /DISCARD/ : {{ *(.eh_frame) }} }}"
            );
            let result = elf::link(
                &[Input {
                    name: "model.o",
                    bytes: &bytes,
                }],
                elf::Options {
                    script: Some(script),
                    ..Default::default()
                },
            );
            let fits = match model {
                CodeModel::Small => address < (1u64 << 32),
                CodeModel::Kernel => {
                    address <= i32::MAX as u64 || address >= i32::MIN as i64 as u64
                }
                _ => unreachable!(),
            };
            if fits {
                result.unwrap();
            } else {
                assert!(
                    result.unwrap_err().contains("overflow"),
                    "{model:?}/{address:#x}"
                );
            }
        }
    }
}
