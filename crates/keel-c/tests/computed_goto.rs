use keel_ir::{InstData, Target, Type};

fn module(src: &str, debug_info: bool, target: Target) -> keel_ir::Module {
    keel_c::compile(
        src,
        "computed-goto.c",
        keel_c::Options { debug_info, target },
    )
    .unwrap()
}

fn switches(m: &keel_ir::Module) -> Vec<(&Vec<i64>, &Vec<keel_ir::BlockCall>)> {
    m.functions
        .iter()
        .flat_map(|f| {
            f.layout_insts().filter_map(|(_, i)| match f.inst_data(i) {
                InstData::Switch { cases, dests, .. } => Some((cases, dests)),
                _ => None,
            })
        })
        .collect()
}

#[test]
fn immutable_dispatch_fuses_before_optimization_with_last_writes_and_zeros() {
    let src = "int run(long i) {\n\
        static const void *const table[6] = {\n\
          [0]=&&b, [1]=&&a, [1]=&&c, [3]=&&b, [4]=&&a, [5]=&&c };\n\
        goto *table[i];\n\
        a: return 10; b: return 20; c: return 30; }";
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for debug in [false, true] {
            for optimize in [false, true] {
                let mut m = module(src, debug, target);
                keel_ir::verify_module(&m).unwrap();
                let tables = switches(&m);
                assert_eq!(tables.len(), 1);
                let (cases, dests) = tables[0];
                assert_eq!(cases, &[0, 1, 3, 4, 5]);
                assert_eq!(dests[1].block, dests[3].block); // b, b
                assert_eq!(dests[2].block, dests[5].block); // c, c
                assert_ne!(dests[2].block, dests[4].block); // overwritten a
                assert!(
                    m.functions.iter().all(|f| f.layout_insts().all(|(_, i)| {
                        !matches!(f.inst_data(i), InstData::Load { ty: Type::Ptr, .. })
                    })),
                    "numeric label table load survived fusion"
                );
                // Other uses and debug data still see exactly the numeric table.
                let init = m.data_object("run.table").unwrap().init.as_ref().unwrap();
                let mut bytes = Vec::new();
                for item in &init.items {
                    match item {
                        keel_ir::DataItem::Bytes(part) => bytes.extend(part),
                        keel_ir::DataItem::Zero(n) => bytes.resize(bytes.len() + *n as usize, 0),
                        _ => panic!("numeric table data"),
                    }
                }
                let labels: Vec<_> = bytes
                    .chunks_exact(8)
                    .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                    .collect();
                assert_eq!(labels, [1, 3, 0, 1, 2, 3]);
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                keel_codegen::compile(&m, target).unwrap();
                keel_codegen::compile_program(&m, target).unwrap();
            }
        }
    }
}

#[test]
fn unsupported_sites_keep_the_whole_function_on_numeric_dispatch() {
    let bodies = [
        // Mutable pointer elements; const pointees alone do not suffice.
        "static const void *table[]={&&a,&&b}; goto *table[i];",
        "static void *const volatile table[]={&&a,&&b}; goto *table[i];",
        "static void *const table[]={&&a,&&b}; void *copy[2]; copy[0]=table[0]; copy[1]=table[1]; goto *copy[i];",
        "static void *const table[]={&&a,&&b}; if(i) goto *table[i]; goto *(&&a + (&&b-&&a));",
        "static void *const table[]={&&a,&&b}; static void *const other[]={&&b,&&a}; if(i) goto *table[i]; goto *other[i];",
        "static void *const table[5]={&&a,&&b}; goto *table[i];", // growth cap
        "static void *const table[]={&&a,(void*)99}; goto *table[i];", // invalid token
        // A hidden goto in the index must also reject the outer candidate.
        "static void *const table[]={&&a,&&b}; goto *table[({ if(i) goto *(&&b); 0; })];",
        // A hidden site in a local initializer cannot be skipped by preflight.
        "static void *const table[]={&&a,&&b}; int x=({if(i) goto *(&&b); 0;}); goto *table[x];",
    ];
    for body in bodies {
        let src = format!("int run(long i) {{ {body} a:return 10; b:return 20; }}");
        for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
            for debug in [false, true] {
                let m = module(&src, debug, target);
                keel_ir::verify_module(&m).unwrap();
                let tables = switches(&m);
                assert_eq!(tables.len(), 1, "{src}");
                assert_eq!(tables[0].0.first(), Some(&1), "{src}");
                assert!(
                    tables[0]
                        .0
                        .iter()
                        .enumerate()
                        .all(|(i, &key)| key == i as i64 + 1),
                    "{src}"
                );
                keel_codegen::compile(&m, target).unwrap();
            }
        }
    }
}
