//! Never execute the adapter in the host process.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{SelectionTier, regalloc::AllocationTier, x86_64::iretq};
use object::{Object, ObjectSection};
#[path = "common/userspace.rs"]
mod userspace;

const SOURCE: &str = include_str!("../../../suite/integration/x86-cpl0/cases/iretq.keel");

#[test]
fn image_contract_and_transport_bytes() {
    let declaration = iretq::declaration();
    assert_eq!(declaration.name, iretq::SYMBOL);
    assert_eq!(declaration.sig, iretq::signature());
    assert!(declaration.attrs.noreturn);
    assert_eq!(std::mem::size_of::<iretq::Image>(), 160);
    assert_eq!(std::mem::align_of::<iretq::Image>(), 8);
    let offsets = [
        std::mem::offset_of!(iretq::Image, rax),
        std::mem::offset_of!(iretq::Image, rcx),
        std::mem::offset_of!(iretq::Image, rdx),
        std::mem::offset_of!(iretq::Image, rbx),
        std::mem::offset_of!(iretq::Image, rbp),
        std::mem::offset_of!(iretq::Image, rsi),
        std::mem::offset_of!(iretq::Image, rdi),
        std::mem::offset_of!(iretq::Image, r8),
        std::mem::offset_of!(iretq::Image, r9),
        std::mem::offset_of!(iretq::Image, r10),
        std::mem::offset_of!(iretq::Image, r11),
        std::mem::offset_of!(iretq::Image, r12),
        std::mem::offset_of!(iretq::Image, r13),
        std::mem::offset_of!(iretq::Image, r14),
        std::mem::offset_of!(iretq::Image, r15),
        std::mem::offset_of!(iretq::Image, rip),
        std::mem::offset_of!(iretq::Image, cs),
        std::mem::offset_of!(iretq::Image, rflags),
        std::mem::offset_of!(iretq::Image, rsp),
        std::mem::offset_of!(iretq::Image, ss),
    ];
    assert_eq!(offsets, std::array::from_fn(|i| i * 8));
    let program = iretq::program();
    assert_eq!(program.stats().text_bytes, 0);
    let object =
        keel_asm::x86::assemble_program_object_with_features(&program, Features::NONE).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&iretq::assembly(), Features::NONE).unwrap()
    );
    let elf = object::File::parse(&*object).unwrap();
    let code = elf.section_by_name(".text").unwrap().data().unwrap();
    // The final sequence switches the stack, restores the image pointer's GPR,
    // and transfers. No callee-save epilogue follows the stack switch.
    assert!(
        code.ends_with(&[0x49, 0x8d, 0x62, 0x78, 0x4d, 0x8b, 0x52, 0x48, 0x48, 0xcf]),
        "{code:02x?}"
    );
    if let Some(dir) = std::env::var_os("KEEL_IRETQ_EXPORT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("iretq-adapter.o"), object).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("iretq-adapter.s"),
            iretq::assembly(),
        )
        .unwrap();
    }
}

#[test]
fn terminal_callers_tiers_and_export() {
    let original = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&original).unwrap();
    assert_eq!(original.decls[0], iretq::declaration());
    let dir = std::env::var_os("KEEL_IRETQ_EXPORT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &dir {
        std::fs::create_dir_all(dir).unwrap();
    }
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for spill in [false, true] {
                    let (bytes, text) = userspace::compile(SOURCE, level, s, a, spill);
                    assert!(text.contains("__keel_x86_iretq"), "{text}");
                    assert!(text.contains("ud2"), "{text}");
                    if level == CpuLevel::V1 {
                        if let Some(dir) = &dir {
                            let name = format!("iretq-{s:?}-{a:?}-{spill}");
                            std::fs::write(dir.join(format!("{name}.o")), bytes).unwrap();
                            std::fs::write(dir.join(format!("{name}.s")), text).unwrap();
                        }
                    }
                }
            }
        }
    }
}
