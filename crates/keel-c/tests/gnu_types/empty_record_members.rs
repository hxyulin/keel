use keel_ir::Target;

fn compile(source: &str, target: Target) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "empty-record-members.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
}

#[test]
fn empty_declarations_preserve_record_layout_and_initialization() {
    let source = r#"
        typedef struct { void *lock; ; } class_preempt_t;
        _Static_assert(sizeof(class_preempt_t) == sizeof(void *), "kernel lock wrapper");
        struct S {
            ;;
            char first;
            ;
            union { ; int integer; ; unsigned int bits; ; };
            ;
            _Static_assert(sizeof(int) == 4, "member assertion");
            struct { ; short low; ; short high; ; };
            char last;
            ;;
        };
        _Static_assert(sizeof(struct S) == 16, "size");
        _Static_assert(_Alignof(struct S) == 4, "alignment");
        _Static_assert(__builtin_offsetof(struct S, first) == 0, "first");
        _Static_assert(__builtin_offsetof(struct S, integer) == 4, "anonymous union");
        _Static_assert(__builtin_offsetof(struct S, low) == 8, "anonymous struct");
        _Static_assert(__builtin_offsetof(struct S, high) == 10, "second short");
        _Static_assert(__builtin_offsetof(struct S, last) == 12, "last");
        struct S initialized = { .first = 1, .integer = 2, .low = 3, .high = 4, .last = 5 };
        int values(void) {
            struct S s = { 1, { 2 }, { 3, 4 }, 5 };
            return s.first + s.integer + s.low + s.high + s.last;
        }
    "#;
    for target in Target::ALL {
        let module = compile(source, target).unwrap_or_else(|e| panic!("{target:?}: {e}"));
        keel_ir::verify_module(&module).unwrap();
    }
}

#[test]
fn empty_members_do_not_create_storage() {
    for target in Target::ALL {
        compile(
            r#"
                struct Empty { ;;; };
                union EmptyUnion { ;;; };
                _Static_assert(sizeof(struct Empty) == 0, "empty struct");
                _Static_assert(_Alignof(struct Empty) == 1, "empty struct alignment");
                _Static_assert(sizeof(union EmptyUnion) == 0, "empty union");
                _Static_assert(_Alignof(union EmptyUnion) == 1, "empty union alignment");
            "#,
            target,
        )
        .unwrap_or_else(|e| panic!("{target:?}: {e}"));
    }
    for target in Target::NATIVE {
        compile(
            r#"
                struct Bits { ; unsigned int a:3; ; unsigned int b:5; ; };
                _Static_assert(sizeof(struct Bits) == 4, "adjacent bit fields");
            "#,
            target,
        )
        .unwrap_or_else(|e| panic!("{target:?}: {e}"));
    }
}

#[test]
fn empty_members_do_not_skip_following_declaration_validation() {
    for target in Target::ALL {
        for source in [
            "struct Bad { ; struct Incomplete member; };",
            "union Bad { ; int member:33; };",
            "struct Bad { ; _Static_assert(0, \"must fail\"); };",
            "struct Bad { ; int member } ;",
        ] {
            assert!(compile(source, target).is_err(), "{target:?}: {source}");
        }
    }
}
