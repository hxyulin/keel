use keel_ir::{InstData, MemoryFlags, Target};

fn compile(source: &str, target: Target) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "memory.c",
        keel_c::Options {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

fn flags(module: &keel_ir::Module, loads: bool) -> Vec<MemoryFlags> {
    module
        .functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap()
        .layout_insts()
        .filter_map(|(_, inst)| {
            let f = module.functions.iter().find(|f| f.name == "f").unwrap();
            match f.inst_data(inst) {
                InstData::Load { flags, .. } if loads => Some(*flags),
                InstData::Store { flags, .. } if !loads => Some(*flags),
                _ => None,
            }
        })
        .collect()
}

#[test]
fn qualifiers_reach_actual_memory_operations_at_each_pointer_depth() {
    let cases: &[(&str, &[bool])] = &[
        ("int f(int *p) { return *p; }", &[false]),
        ("int f(volatile int *p) { return *p; }", &[true]),
        ("int f(int *volatile p) { return *p; }", &[true, false]),
        (
            "int f(volatile int *volatile p) { return *p; }",
            &[true, true],
        ),
        ("int f(volatile int **p) { return **p; }", &[false, true]),
        (
            "int f(int *volatile p) { typeof(p + 0) q = p; return *q; }",
            &[true, false],
        ),
        (
            "int f(int c, int *volatile p, int *q) { typeof(c ? p : q) r = p; return *r; }",
            &[true, false],
        ),
        (
            "int f(int *volatile p) { typeof((0, p)) q = p; return *q; }",
            &[true, false],
        ),
        (
            "int f(int *volatile p) { typeof(({p;})) q = p; return *q; }",
            &[true, false],
        ),
        (
            "int f(int *volatile p) { typeof((p = p)) q = p; return *q; }",
            &[true, false],
        ),
        (
            "typedef volatile int V; int f(V *p) { return *p; }",
            &[true],
        ),
        (
            "typedef volatile int V; int f(void) { V x = 1; return x; }",
            &[true],
        ),
        (
            "typedef volatile int A[2]; int f(A p) { return p[0]; }",
            &[true],
        ),
        (
            "struct S { int x; }; int f(volatile struct S *p) { return p->x; }",
            &[true],
        ),
        (
            "struct S { volatile int x; int y; }; int f(struct S *p) { return p->y; }",
            &[false],
        ),
        (
            "struct S { volatile int x; }; int f(struct S *p) { return p->x; }",
            &[true],
        ),
        (
            "struct S { volatile struct { int x; }; }; int f(struct S *p) { return p->x; }",
            &[true],
        ),
        ("int f(int *p) { return *(volatile int *)p; }", &[true]),
        ("int f(volatile int *p) { return *(int *)p; }", &[false]),
        (
            "int f(int *volatile p) { return *(int *)p; }",
            &[true, false],
        ),
        (
            "int f(int c, volatile int *p, int *q) { return *(c ? p : q); }",
            &[true],
        ),
        (
            "int f(volatile int *p) { return *(p ?: (int *)0); }",
            &[true],
        ),
        ("int f(volatile int *p) { return *({ p; }); }", &[true]),
        ("int f(volatile int *p) { return *(0, p); }", &[true]),
        (
            "int f(volatile int *p) { __auto_type q = p; return *q; }",
            &[true],
        ),
        (
            "int f(volatile int *p) { typeof(*p) x = 1; return x; }",
            &[true],
        ),
        (
            "volatile int *get(void); int f(void) { return *get(); }",
            &[true],
        ),
        ("int f(_Atomic(volatile int *) *p) { return **p; }", &[true]),
        (
            "int f(_Atomic(volatile int *) *p) { return *__atomic_load_n(p, 5); }",
            &[true],
        ),
    ];
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for &(source, expected) in cases {
            let mut module = compile(source, target);
            assert_eq!(
                flags(&module, true)
                    .iter()
                    .map(|f| f.volatile)
                    .collect::<Vec<_>>(),
                expected,
                "{source}\n{module}"
            );
            keel_opt::optimize(&mut module).unwrap();
            assert_eq!(
                flags(&module, true)
                    .iter()
                    .map(|f| f.volatile)
                    .collect::<Vec<_>>(),
                expected,
                "optimized {source}\n{module}"
            );
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

#[test]
fn volatile_writes_bitfields_and_large_aggregate_copies_stay_explicit() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for source in [
            "int f(volatile int *p) { *p = 1; return 0; }",
            "struct S { volatile unsigned x:3; }; int f(struct S *p) { p->x = 1; return p->x; }",
            "int f(volatile int *p) { return __builtin_sadd_overflow(1, 2, p); }",
        ] {
            let mut module = compile(source, target);
            assert!(
                flags(&module, false).iter().any(|f| f.volatile),
                "{source}\n{module}"
            );
            keel_opt::optimize(&mut module).unwrap();
            assert!(flags(&module, false).iter().any(|f| f.volatile));
            keel_codegen::compile(&module, target).unwrap();
        }
        for source in [
            "struct S { long x[10]; }; int f(struct S *d, volatile struct S *s) { *d = *s; return 0; }",
            "struct S { long x[10]; }; int f(volatile struct S *d, struct S *s) { *d = *s; return 0; }",
            "struct S { long x[10]; }; int f(void) { volatile struct S s = {{1, 2}}; return s.x[0]; }",
        ] {
            let mut module = compile(source, target);
            // Ordinary copies from a materialized temporary may still use
            // memcpy, but the original volatile region must be explicit.
            if source.contains("volatile struct S *s") {
                assert_eq!(
                    flags(&module, true).iter().filter(|f| f.volatile).count(),
                    10
                );
                let f = module.functions.iter().find(|f| f.name == "f").unwrap();
                for (_, inst) in f.layout_insts() {
                    if let InstData::Intrinsic {
                        id: keel_ir::Intrinsic::Memcpy | keel_ir::Intrinsic::Memmove,
                        args,
                    } = f.inst_data(inst)
                    {
                        let keel_ir::ValueDef::Result(def, _) = f.value_def(f.resolve(args[1]))
                        else {
                            panic!("volatile region passed to memcpy");
                        };
                        assert!(matches!(f.inst_data(def), InstData::StackAddr { .. }));
                    }
                }
            } else {
                assert!(
                    !module.to_string().contains("intrinsic memcpy"),
                    "{source}\n{module}"
                );
            }
            assert!(
                flags(&module, true)
                    .iter()
                    .chain(flags(&module, false).iter())
                    .any(|f| f.volatile)
            );
            keel_opt::optimize(&mut module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

#[test]
fn oversized_local_objects_are_diagnosed_instead_of_truncated() {
    let error = keel_c::compile(
        "int f(void) { char x[4294967296UL]; return x[0]; }",
        "large.c",
        Default::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("stack-slot size limit"));
}

#[test]
fn literal_casts_still_fold_without_enabling_ir_optimization() {
    let module = compile(
        "unsigned char f(void) { return (unsigned char)257; }",
        Target::Aarch64AppleDarwin,
    );
    assert!(!module.to_string().contains("trunc"));
    assert!(module.to_string().contains("iconst.i8 1"));
}

#[test]
fn large_aggregate_assignment_does_not_assume_disjoint_regions() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let module = compile(
            "struct S { long x[10]; }; void f(struct S *d, struct S *s) { *d = *s; }",
            target,
        );
        assert!(module.to_string().contains("intrinsic memmove"));
        assert!(!module.to_string().contains("intrinsic memcpy"));
    }
}

#[test]
fn byval_arguments_are_snapshotted_before_later_effects() {
    let source = "struct S { long x, y; }; int mutate(void); int use(struct S, int); int f(struct S *p) { return use(*p, mutate()); }";
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let mut module = compile(source, target);
        for optimize in [false, true] {
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            let f = module.functions.iter().find(|f| f.name == "f").unwrap();
            let mut passed_effect = false;
            let mut copied_before_effect = 0;
            for (_, inst) in f.layout_insts() {
                match f.inst_data(inst) {
                    InstData::Call { func, .. } if f.callee(*func).name == "mutate" => {
                        passed_effect = true
                    }
                    InstData::Store { .. } if !passed_effect => copied_before_effect += 1,
                    InstData::Call { func, args } if f.callee(*func).name == "use" => {
                        let keel_ir::ValueDef::Result(def, _) = f.value_def(f.resolve(args[0]))
                        else {
                            panic!("no byval snapshot");
                        };
                        assert!(matches!(f.inst_data(def), InstData::StackAddr { .. }));
                    }
                    _ => {}
                }
            }
            assert!(passed_effect && copied_before_effect >= 2, "{module}");
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

#[test]
fn over_aligned_volatile_locals_preserve_access_flags() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let module = compile(
            "int f(void) { volatile int x __attribute__((aligned(64))) = 3; return x; }",
            target,
        );
        assert!(flags(&module, false).iter().any(|f| f.volatile));
        assert!(flags(&module, true).iter().any(|f| f.volatile));
        keel_codegen::compile(&module, target).unwrap();
        let module = compile(
            "struct __attribute__((aligned(64))) S { int x; }; int f(void) { volatile struct S a = {3}; struct S b; b = a; return b.x; }",
            target,
        );
        assert!(flags(&module, true).iter().any(|f| f.volatile));
        keel_codegen::compile(&module, target).unwrap();
    }
}
