//! Compile-only compatibility matrix; no emitted privileged code executes.
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use keel_ir::Intrinsic;

#[path = "common/userspace.rs"]
mod userspace;

fn source(include_gaps: bool) -> String {
    let mut source = String::new();
    let rows = include_str!("../../keel-ir/tests/fixtures/x86-query-bindings.txt");
    for row in rows.lines().filter(|s| !s.starts_with('#')) {
        let fields: Vec<_> = row.split('|').collect();
        // Existing wide LAR/LSL register forms fail the operand matcher before
        // this metadata migration. Keep them in the metadata fixture, outside
        // this successful-object matrix. See docs/x86-query-bindings.md.
        if !include_gaps && matches!(fields[0], "703" | "705" | "709" | "711") {
            continue;
        }
        let id = Intrinsic::from_name(fields[2]).unwrap();
        let info = id.info();
        let mut types: Vec<_> = info.params.iter().map(ToString::to_string).collect();
        types.extend(["ptr".into(), "ptr".into(), "ptr".into()]);
        source += &format!(
            "func @binding_{}({}) {{\nentry({}):\n",
            fields[0],
            types.join(","),
            types
                .iter()
                .enumerate()
                .map(|(i, t)| format!("v{i}:{t}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut args: Vec<_> = (0..info.params.len()).map(|i| format!("v{i}")).collect();
        let base = types.len();
        if id.x86_port_io().is_some_and(|(_, _, immediate)| immediate) {
            source += &format!(" v{base} = iconst.i16 224\n");
            args[0] = format!("v{base}");
        }
        let results: Vec<_> = (0..info.returns.len())
            .map(|i| format!("v{}", base + 1 + i))
            .collect();
        source += &format!(
            " {}intrinsic {}({})\n",
            if results.is_empty() {
                String::new()
            } else {
                format!("{} = ", results.join(","))
            },
            info.name,
            args.join(",")
        );
        for (i, value) in results.iter().enumerate() {
            source += &format!(" store {value},v{}\n", info.params.len() + i);
        }
        // Retain every original input across the operation, including old
        // destinations and the dynamic input corresponding to an immediate.
        for i in 0..info.params.len() {
            source += &format!(" store v{i},v{}\n", info.params.len() + 2);
        }
        source += " return\n}\n";
    }
    assert_eq!(
        source.matches("func @").count(),
        if include_gaps { 42 } else { 38 }
    );
    source
}

#[test]
fn query_and_scalar_io_binding_transport_matrix() {
    let source = source(false);
    let export = std::env::var_os("KEEL_QUERY_BINDINGS_EXPORT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("bindings.ir"), &source).unwrap();
    }
    let mut count = 0;
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for selector in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                for empty in [false, true] {
                    let (object, text) =
                        userspace::compile(&source, level, selector, allocator, empty);
                    if let Some(dir) = &export {
                        std::fs::write(dir.join(format!("matrix-{count:02}.o")), object).unwrap();
                        std::fs::write(dir.join(format!("matrix-{count:02}.s")), text).unwrap();
                    }
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 36);
}

#[test]
fn all_query_bindings_reach_typed_lowering_and_reject_foreign_targets() {
    let module = keel_ir::parse(&source(true)).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(
        &module,
        &keel_codegen::x86_64::REGISTERS,
        &keel_codegen::CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    assert!(keel_codegen::aarch64::compile(&module).is_err());
}
