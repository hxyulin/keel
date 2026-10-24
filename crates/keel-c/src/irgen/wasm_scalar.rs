//! Private integer/pointer storage lowering checkpoint, not a C ABI or production target.
use super::*;

fn reject(loc: Loc, family: &str) -> Error {
    Error::new(
        loc,
        format!("private wasm32 scalar profile rejects {family}"),
    )
}

fn integer(ty: &Ty) -> bool {
    ty.int_kind()
        .is_some_and(|k| k.size_with_layout(CDataLayout::WASM32) <= 8)
}

fn signature(fty: &FuncTy, loc: Loc) -> R<()> {
    if !fty.prototyped || fty.variadic {
        return Err(reject(loc, "unprototyped/variadic signatures"));
    }
    if std::iter::once(&fty.ret).chain(fty.params).any(|ty| {
        !matches!(ty, Ty::Void)
            && !ty
                .int_kind()
                .is_some_and(|k| matches!(k.size_with_layout(CDataLayout::WASM32), 4 | 8))
    }) {
        return Err(reject(loc, "signatures other than plain i32/i64"));
    }
    Ok(())
}

fn expression(e: &Expr<'_>) -> R<()> {
    expression_use(e, false)
}

fn expression_use(e: &Expr<'_>, storage: bool) -> R<()> {
    let mut pointer = e.ty;
    while let Ty::Ptr(inner) = pointer {
        if matches!(inner, Ty::Func(_)) {
            return Err(reject(e.loc, "function pointers"));
        }
        pointer = *inner;
    }
    if e.ty != Ty::Void && !integer(&e.ty) && !matches!(e.ty, Ty::Ptr(_)) && !storage {
        return Err(reject(e.loc, "aggregate/unsupported values"));
    }
    match e.kind {
        ExprKind::Int(_) | ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::Str(_) => Ok(()),
        ExprKind::Addr(a) => {
            if matches!(a.ty, Ty::Func(_)) {
                return Err(reject(e.loc, "function pointers"));
            }
            expression_use(a, true)
        }
        ExprKind::Deref(a) => expression(a),
        ExprKind::Member {
            base, bits: None, ..
        } => expression_use(base, true),
        ExprKind::PtrAdd(a, b, _)
        | ExprKind::PtrDiff(a, b, _)
        | ExprKind::PtrOpAssign { lhs: a, rhs: b, .. } => {
            expression(a)?;
            expression(b)
        }
        ExprKind::Unary(_, a) | ExprKind::Cast(a) | ExprKind::IncDec { lhs: a, .. } => {
            expression(a)
        }
        ExprKind::Binary(_, a, b)
        | ExprKind::LogAnd(a, b)
        | ExprKind::LogOr(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::Comma(a, b)
        | ExprKind::OpAssign { lhs: a, rhs: b, .. } => {
            expression(a)?;
            expression(b)
        }
        ExprKind::Cond(c, a, b) => {
            expression(c)?;
            expression(a)?;
            expression(b)
        }
        ExprKind::Call(callee, args) => {
            // Only the parser's direct function decay may produce a pointer.
            let ExprKind::Addr(global) = callee.kind else {
                return Err(reject(e.loc, "indirect calls"));
            };
            let (ExprKind::Global(_), Ty::Func(fty)) = (global.kind, global.ty) else {
                return Err(reject(e.loc, "indirect calls"));
            };
            signature(fty, e.loc)?;
            for arg in args {
                expression(arg)?;
            }
            Ok(())
        }
        _ => Err(reject(e.loc, "unmigrated expressions/builtins")),
    }
}

fn statement(s: &Stmt<'_>, sizes: &[u64]) -> R<()> {
    match s {
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => expression(e),
        Stmt::Init(id, init, loc) => {
            if sizes[*id] > 64 {
                return Err(reject(
                    *loc,
                    "local initializers above 64 bytes (memory intrinsic path)",
                ));
            }
            for item in init.items {
                match item {
                    InitItem::Expr {
                        expr, bits: None, ..
                    } => expression(expr)?,
                    InitItem::Bytes { .. } => return Err(reject(*loc, "local byte initializers")),
                    _ => return Err(reject(*loc, "bit-field initializers")),
                }
            }
            Ok(())
        }
        Stmt::Block(ss) | Stmt::Scoped(_, ss) => statements(ss, sizes),
        Stmt::If(e, yes, no) => {
            expression(e)?;
            statement(yes, sizes)?;
            if let Some(no) = no {
                statement(no, sizes)?;
            }
            Ok(())
        }
        Stmt::While(e, body) | Stmt::DoWhile(body, e) => {
            expression(e)?;
            statement(body, sizes)
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => {
            if let Some(init) = init {
                statement(init, sizes)?;
            }
            if let Some(cond) = cond {
                expression(cond)?;
            }
            if let Some(step) = step {
                expression(step)?;
            }
            statement(body, sizes)
        }
        Stmt::Switch { cond, body, .. } => {
            expression(cond)?;
            statement(body, sizes)
        }
        Stmt::Declare(_)
        | Stmt::Case(_)
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Return(None, _)
        | Stmt::Goto(_)
        | Stmt::Label(_) => Ok(()),
        _ => Err(reject(Loc::default(), "assembly/computed goto")),
    }
}

fn statements(ss: &[Stmt<'_>], sizes: &[u64]) -> R<()> {
    for s in ss {
        statement(s, sizes)?;
    }
    Ok(())
}

impl Gen<'_> {
    fn private_storage(&self, ty: &Ty<'_>, seen: &mut Vec<usize>) -> R<()> {
        match ty {
            ty if integer(ty) => Ok(()),
            Ty::Ptr(inner) if matches!(inner, Ty::Void) => Ok(()),
            Ty::Ptr(inner) => self.private_storage(inner, seen),
            Ty::Array(a) if a.vla.is_none() && a.len.is_some() => {
                self.private_storage(&a.elem, seen)?;
                // Check each dimension before Types::size's multiplication.
                self.types()
                    .size(&a.elem)
                    .checked_mul(a.len.unwrap())
                    .filter(|size| *size <= u32::MAX as u64)
                    .ok_or_else(|| reject(Loc::default(), "array storage exceeds wasm32 bounds"))?;
                Ok(())
            }
            Ty::Struct(id) if seen.contains(id) => Ok(()),
            Ty::Struct(id) => {
                seen.push(*id);
                let record = &self.p.structs[*id];
                if !record.complete
                    || record.size > u32::MAX as u64
                    || record.members.iter().any(|m| m.bits.is_some())
                {
                    return Err(reject(Loc::default(), "incomplete/bit-field storage"));
                }
                for m in &record.members {
                    self.private_storage(&m.ty, seen)?;
                }
                Ok(())
            }
            _ => Err(reject(
                Loc::default(),
                "unsupported storage (function pointers/FP/atomic/vector/VLA)",
            )),
        }
    }

    fn private_data(&self, g: &Global<'_>) -> R<()> {
        let size = self.types().size(&g.ty);
        if size == 0 || size > u32::MAX as u64 || g.align > 4096 {
            return Err(reject(Loc::default(), "data size/alignment"));
        }
        let mut spans = Vec::new();
        for item in g.init.as_ref().map_or(&[][..], |i| i.items) {
            let (offset, width) = match item {
                InitItem::Bytes { offset, bytes } => (*offset, bytes.len() as u64),
                InitItem::Expr {
                    offset,
                    expr,
                    bits: None,
                } => {
                    expression(expr)?;
                    let width = self.types().size(&expr.ty);
                    match crate::eval::eval_with_binding_and_layout(
                        expr,
                        &|id| self.p.globals[id].weak.is_none(),
                        self.p.c_layout,
                    ) {
                        Some(Const::Int(_))
                            if integer(&expr.ty) || matches!(expr.ty, Ty::Ptr(_)) => {}
                        Some(Const::Addr(base, addend)) if matches!(expr.ty, Ty::Ptr(_)) => {
                            let target_size = match base {
                                Base::Global(id)
                                    if self.p.globals[id].defined
                                        && !matches!(self.p.globals[id].ty, Ty::Func(_)) =>
                                {
                                    self.types().size(&self.p.globals[id].ty)
                                }
                                // String relocations need a separately checked string placement contract.
                                _ => {
                                    return Err(reject(
                                        expr.loc,
                                        "symbolic target other than defined data",
                                    ));
                                }
                            };
                            if width != 4
                                || g.align < 4
                                || offset % 4 != 0
                                || addend < 0
                                || addend as u64 > target_size
                            {
                                return Err(reject(
                                    expr.loc,
                                    "pointer initializer alignment/bounds",
                                ));
                            }
                        }
                        _ => {
                            return Err(reject(
                                expr.loc,
                                "nonconstant/nonpointer symbolic initializer",
                            ));
                        }
                    }
                    (*offset, width)
                }
                _ => return Err(reject(Loc::default(), "bit-field initializer")),
            };
            let end = offset
                .checked_add(width)
                .filter(|end| *end <= size)
                .ok_or_else(|| reject(Loc::default(), "initializer bounds"))?;
            if spans.iter().any(|&(a, b)| offset < b && a < end) {
                return Err(reject(Loc::default(), "overlapping initializers"));
            }
            spans.push((offset, end));
        }
        Ok(())
    }

    pub(super) fn check_private_scalar_profile(&self) -> R<()> {
        if self.debug {
            return Err(reject(Loc::default(), "debug output"));
        }
        for g in &self.p.globals {
            if g.compound_literal {
                return Err(reject(Loc::default(), "file-scope compound literals"));
            }
            if let Ty::Func(fty) = g.ty {
                signature(fty, Loc::default())?;
            } else {
                if !g.defined {
                    return Err(reject(Loc::default(), "external data"));
                }
                self.private_storage(&g.ty, &mut Vec::new())?;
                self.private_data(g)?;
            }
            if g.thread_local
                || g.returns_twice
                || g.weak.is_some()
                || g.lifecycle.is_some()
                || g.section.is_some()
                || g.retain.is_some()
                || g.visibility.is_some()
                || g.used
                || g.function_attrs != keel_ir::FunctionAttrs::default()
            {
                return Err(reject(Loc::default(), "native linkage/function attributes"));
            }
        }
        // This runs before Gen's worklist/pruning, including unused bodies.
        for def in self.funcs.iter().flatten() {
            signature(def.ty, def.loc)?;
            if def.has_vm || !def.label_addrs.is_empty() {
                return Err(reject(def.loc, "VLA/label addresses"));
            }
            for local in &def.locals {
                self.private_storage(&local.ty, &mut Vec::new())?;
                if local.cleanup.is_some() {
                    return Err(reject(
                        def.loc,
                        "non-integer/atomic/aggregate locals or cleanup",
                    ));
                }
                if local.align > 16 {
                    return Err(reject(def.loc, "local alignment above 16"));
                }
            }
            let sizes: Vec<_> = def
                .locals
                .iter()
                .map(|l| self.types().size(&l.ty))
                .collect();
            statements(def.body, &sizes)?;
            // Inspect pointer pointee storage as well, including casts hidden in
            // unused expressions. Direct function decay is checked by Call.
            let mut error = None;
            for stmt in def.body {
                visit_stmt_exprs(stmt, &mut |e, _| {
                    if matches!(e.ty, Ty::Ptr(_))
                        && !matches!(e.kind, ExprKind::Addr(a) if matches!(a.ty, Ty::Func(_)))
                    {
                        if let Err(err) = self.private_storage(&e.ty, &mut Vec::new()) {
                            error = Some(err);
                        }
                    }
                    Some(())
                });
            }
            if let Some(err) = error {
                return Err(err);
            }
        }
        Ok(())
    }
}

/// Accepts preprocessed/header-free input only. The borrowed native parser
/// target supplies no Wasm macros, headers or calling convention. No optimizer.
pub(crate) fn lower(src: &str, debug: bool) -> R<Module> {
    let arena = bumpalo::Bump::new();
    let mut p = Parser::declarations_with_layout(
        src,
        crate::Options::default().target,
        crate::strings::Strings::new(&arena),
        CDataLayout::WASM32,
    )?;
    let files = p.take_files();
    let funcs = std::mem::take(&mut p.funcs);
    let module = Gen::new(&p, funcs, &files, debug).run()?;
    keel_ir::verify_module_with_layout(&module, keel_ir::DataLayout::WASM32).map_err(|e| {
        Error::new(
            Loc::default(),
            format!("private wasm32 IR verification: {e:?}"),
        )
    })?;
    Ok(module)
}

#[test]
fn widths_calls_and_storage() {
    let module = lower("long helper(long x) { return x + 1; } long f(long x, unsigned long u, long long y) { volatile long slot = x; short s = (short)y; slot += s; return helper(slot) + (long)(u >> 31); }", false).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let sig = module.signature("f").unwrap();
    assert_eq!(
        sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
        [Type::I32, Type::I32, Type::I64]
    );
    assert_eq!(sig.returns[0].ty, Type::I32);
    assert!(
        sig.params
            .iter()
            .all(|p| p.ext == ArgExt::None && p.purpose == Purpose::Normal)
    );
    let f = module.functions.iter().find(|f| f.name == "f").unwrap();
    assert!(f.stack_slots().any(|(_, s)| s.size == 4 && s.align == 4));
    let text = module.to_string();
    assert!(text.contains("trunc"), "{text}");
    assert!(text.contains("sext"), "{text}");
    assert!(text.contains("ushr"), "{text}");
    assert!(text.contains("call"), "{text}");
    assert!(module.data.is_empty());
}

#[test]
fn reject_before_pruning() {
    for src in [
        "static int unused(void) { float x = 1; return (int)x; } int f(void) { return 0; }",
        "static int unused(void) { _Atomic int x; return 0; }",
        "static int unused(int n) { int a[n]; return 0; }",
        "static int unused(void) { int x __attribute__((aligned(32))); return 0; }",
        "static int unused(void) { return __builtin_popcount(1); }",
        "static int unused(void) { __asm__(\"\"); return 0; }",
        "int f(double x);",
        "int f(__int128 x);",
        "static int unused(void) { return __builtin_frame_address(0) != 0; }",
        "static int unused(void) { if (0) return __builtin_popcount(1); return 0; }",
        "int f(char x);",
        "int f(int x, ...);",
        "int f(void) { return (int)(double)1; }",
    ] {
        assert!(lower(src, false).is_err(), "accepted {src}");
    }
    assert!(
        lower("int f(void) { return 0; }", true)
            .unwrap_err()
            .message
            .contains("debug")
    );
}

#[test]
fn maintained_fixture_and_native_control() {
    let src = include_str!("../../tests/fixtures/wasm-scalar-lowering.c");
    let wasm = lower(src, false).unwrap();
    keel_ir::verify_module(&wasm).unwrap();
    let native = crate::compile(src, "scalar.c", crate::Options::default()).unwrap();
    keel_ir::verify_module(&native).unwrap();
    assert_eq!(wasm.signature("widths").unwrap().returns[0].ty, Type::I32);
    assert_eq!(native.signature("widths").unwrap().returns[0].ty, Type::I64);
    let function = |m: &Module, name: &str| {
        m.functions
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .to_string()
    };
    assert!(function(&wasm, "folded").contains("iconst.i32 1"));
    assert!(function(&native, "folded").contains("iconst.i64 4294967297"));
    assert!(function(&wasm, "mixed").contains("udiv "));
    assert!(function(&native, "mixed").contains("sdiv "));
    assert!(function(&wasm, "widen").contains("uext.i64"));
    assert!(function(&wasm, "truth").contains("trunc.i32"));
    assert!(!function(&native, "truth").contains("trunc.i32"));
    assert!(!function(&wasm, "mask").contains("and "));
    for target in [Some("wasm32-unknown-unknown"), None] {
        let mut cmd = std::process::Command::new("clang");
        if let Some(target) = target {
            cmd.arg(format!("--target={target}"));
        }
        let output = cmd
            .args([
                "-std=gnu11",
                "-ffreestanding",
                "-nostdinc",
                "-Werror",
                "-fsyntax-only",
            ])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/wasm-scalar-lowering.c"
            ))
            .output()
            .expect("existing Clang required for declaration/type oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn oversized_switch_range_is_diagnostic() {
    let src = "int f(long long x) { switch (x) { case (-9223372036854775807LL-1) ... 9223372036854775807LL: return 1; default: return 0; } }";
    let err = lower(src, false).unwrap_err();
    assert!(
        err.message.contains("case range is empty or too big"),
        "{err:?}"
    );
    let err = crate::compile(src, "range.c", crate::Options::default()).unwrap_err();
    assert!(
        err.message.contains("case range is empty or too big"),
        "{err:?}"
    );
}

#[test]
fn clang_constant_oracle_and_runtime_shapes() {
    let src = include_str!("../../tests/fixtures/wasm-scalar-lowering.c");
    let wasm = lower(src, false).unwrap();
    let native = crate::compile(src, "scalar.c", crate::Options::default()).unwrap();
    for (module, target) in [(&wasm, Some("wasm32-unknown-unknown")), (&native, None)] {
        keel_ir::verify_module(module).unwrap();
        let mut cmd = std::process::Command::new("clang");
        if let Some(target) = target {
            cmd.arg(format!("--target={target}"));
        }
        let output = cmd
            .args([
                "-std=gnu11",
                "-ffreestanding",
                "-nostdinc",
                "-Werror",
                "-O1",
                "-S",
                "-emit-llvm",
                "-o",
                "-",
            ])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/wasm-scalar-lowering.c"
            ))
            .output()
            .expect("existing Clang required for integer oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let llvm = String::from_utf8(output.stdout).unwrap();
        for name in [
            "oracle_div",
            "oracle_rem",
            "oracle_shift",
            "oracle_compare",
            "oracle_rank",
            "oracle_cast",
            "oracle_signed",
            "oracle_bool",
            "oracle_promote",
        ] {
            // Read Clang's independently optimized return, then inspect the
            // actual value returned by Keel (not any incidental constant).
            let body = llvm
                .split(&format!("@{name}("))
                .nth(1)
                .unwrap()
                .split('}')
                .next()
                .unwrap();
            let want: i64 = body
                .lines()
                .find_map(|line| line.trim().strip_prefix("ret i64 "))
                .unwrap()
                .parse()
                .unwrap();
            let f = module.functions.iter().find(|f| f.name == name).unwrap();
            let returns: Vec<_> = f
                .blocks()
                .flat_map(|b| f.block_insts(b))
                .filter_map(|&inst| {
                    if let InstData::Return { args: values } = f.inst_data(inst) {
                        assert_eq!(values.len(), 1);
                        let ValueDef::Result(constant, _) = f.value_def(f.resolve(values[0]))
                        else {
                            panic!("{name} did not return a constant: {f}");
                        };
                        let InstData::Iconst { ty: Type::I64, imm } = f.inst_data(constant) else {
                            panic!("{name} did not return i64 constant: {f}");
                        };
                        Some(*imm)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(returns, [want], "{name}, target {target:?}: {f}");
        }
        // All declaration, definition and direct-call ABI entries stay plain.
        for name in ["helper", "widths", "slots", "signed_widen", "phi"] {
            let sig = module.signature(name).unwrap();
            assert!(!sig.variadic && !sig.returns_twice);
            assert!(
                sig.params
                    .iter()
                    .chain(&sig.returns)
                    .all(|p| p.purpose == Purpose::Normal && p.ext == ArgExt::None)
            );
        }
    }
    let function = |name: &str| wasm.functions.iter().find(|f| f.name == name).unwrap();
    let slots: Vec<_> = function("slots")
        .stack_slots()
        .map(|(_, s)| (s.size, s.align))
        .collect();
    assert_eq!(slots, [(1, 1), (2, 2), (4, 4), (8, 8)]);
    for (name, operation) in [
        ("signed_widen", "sext.i64"),
        ("signed_shift", "sshr "),
        ("unsigned_shift", "ushr "),
        ("truth_not", "trunc.i32"),
        ("truth_and", "trunc.i32"),
    ] {
        let text = function(name).to_string();
        assert!(text.contains(operation), "{name}: {text}");
    }
    let defaults = lower("long default_return(void) { }", false).unwrap();
    keel_ir::verify_module(&defaults).unwrap();
    assert!(defaults.functions[0].to_string().contains("iconst.i32 0"));
    assert!(
        function("phi")
            .blocks()
            .any(|b| !function("phi").block_params(b).is_empty())
    );
}

#[test]
fn rejection_families_and_prototype_policy() {
    for (src, diagnostic) in [
        (
            "int f(void) { if (0) { (void)&f; } return 0; }",
            "function pointers",
        ),
        (
            "int f(void) { return 0 ? __builtin_popcount(1) : 0; }",
            "unmigrated expressions/builtins",
        ),
        (
            "int f(void) { return 0 && __builtin_popcount(1); }",
            "unmigrated expressions/builtins",
        ),
        ("int f(void) { return unknown(1); }", "is not declared"),
        (
            "int f(); int g(void) { return 0; }",
            "unprototyped/variadic",
        ),
        ("short f(void);", "plain i32/i64"),
        (
            "int f(void) __attribute__((noreturn));",
            "native linkage/function attributes",
        ),
        (
            "int f(void) __attribute__((weak));",
            "native linkage/function attributes",
        ),
        (
            "int f(void) __attribute__((visibility(\"hidden\")));",
            "native linkage/function attributes",
        ),
        (
            "int f(void) { return ({ 1; }); }",
            "unmigrated expressions/builtins",
        ),
        (
            "int f(void) { return (int){1}; }",
            "unmigrated expressions/builtins",
        ),
        (
            "int f(void) { int (*p)(void) = f; return p(); }",
            "unsupported storage",
        ),
    ] {
        let err = lower(src, false).unwrap_err();
        assert!(err.message.contains(diagnostic), "{src}: {err:?}");
    }
    // A known plain prototype is IR declaration knowledge, never an import grant.
    let module = lower("unsigned long ext(unsigned long, long long); unsigned long f(unsigned long x) { return ext(x, 7LL); }", false).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let sig = module.signature("ext").unwrap();
    assert_eq!(
        sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
        [Type::I32, Type::I64]
    );
    assert_eq!(sig.returns[0].ty, Type::I32);
    assert!(
        sig.params
            .iter()
            .chain(&sig.returns)
            .all(|p| p.ext == ArgExt::None && p.purpose == Purpose::Normal)
    );
    assert!(module.data.is_empty());
}

#[test]
fn public_native_options_keep_lp64_and_arg_extensions() {
    // Literal construction also protects the existing public Options shape.
    for target in keel_ir::Target::NATIVE {
        for debug_info in [false, true] {
            let module = crate::compile(
                "long width(long x) { volatile long y = x; return y; } signed char narrow(signed char x) { return x; }",
                "native.c", crate::Options { target, debug_info },
            ).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let sig = module.signature("width").unwrap();
            assert_eq!(sig.params[0].ty, Type::I64);
            assert_eq!(sig.returns[0].ty, Type::I64);
            let width = module.functions.iter().find(|f| f.name == "width").unwrap();
            assert!(
                width
                    .stack_slots()
                    .any(|(_, s)| s.size == 8 && s.align == 8)
            );
            let sig = module.signature("narrow").unwrap();
            assert!(
                sig.params
                    .iter()
                    .chain(&sig.returns)
                    .all(|p| p.ty == Type::I8
                        && p.ext == ArgExt::Sext
                        && p.purpose == Purpose::Normal)
            );
        }
    }
}

#[test]
fn pointer_data_runtime() {
    use keel_wasm::{
        Export, Options, Wasm32MemoryOptions, Wasm32RuntimeOptions, emit_wasm32_runtime_module,
    };
    use std::io::Write;
    use std::process::{Command, Stdio};
    let source = include_str!("../../tests/fixtures/wasm-pointer-data.c");
    // The private seam accepts preprocessed C; these are only model assertions.
    let body = source.split_once("struct Storage").unwrap().1;
    let source = format!("struct Storage{body}");
    let module = lower(&source, false).unwrap();
    let names = [
        "checksum",
        "recurse",
        "distance",
        "pointer_bits",
        "numeric_bits",
        "big_difference",
        "pointer_slot",
        "constant_pointer",
        "walk",
        "ordered",
        "stride",
        "local_init",
    ];
    let bytes = emit_wasm32_runtime_module(
        &module,
        &Wasm32RuntimeOptions {
            memory: Wasm32MemoryOptions {
                scalar: Options {
                    exports: names
                        .iter()
                        .map(|name| Export {
                            name: (*name).into(),
                            function: (*name).into(),
                        })
                        .collect(),
                },
                pages: 1,
                export_name: "memory".into(),
            },
            data_floor: 1024,
            stack_bottom: 32768,
            stack_top: 65536,
        },
    )
    .unwrap_or_else(|e| panic!("{e:?}\n{module}"));
    let script = r#"
const assert = require('assert/strict');
const b = require('fs').readFileSync(0);
assert(WebAssembly.validate(b));
const e = new WebAssembly.Instance(new WebAssembly.Module(b)).exports;
const memory = new Uint8Array(e.memory.buffer);
memory.fill(0xcc, 8191, 8225);
for (let i=0; i<32; i++) memory[8192+i]=(i*17+129)&255;
let want=7;
for (let i=0; i<32; i++) want=(Math.imul(want,33)^memory[8192+i])>>>0;
for (let i=0; i<16; i++) want=(want^((i^0x5a)<<(i&7)))>>>0;
want=(want+97+22+165+90+165+90)>>>0;
for (let i=0; i<100; i++) {
 assert.equal(e.checksum(8192,32)>>>0,want);
 assert.equal(e.recurse(10),55);
 assert.equal(e.pointer_slot(),22);
}
assert.equal(memory[8191],0xcc); assert.equal(memory[8224],0xcc);
assert.equal(e.distance(),3);
assert.equal(e.stride(),2);
assert.equal(e.local_init(),14);
assert.equal(e.walk(8193,-1),129);
assert.equal(e.ordered(0x80000000,0x7fffffff),1);
assert.equal(e.constant_pointer(),0x80000001n);
assert.equal(e.numeric_bits(),0x80000001n);
assert.equal(e.pointer_bits(0x1234567880000001n),0x80000001n);
assert.equal(e.pointer_bits(-1n),0xffffffffn);
// Arithmetic inspection fixture: offsets are not dereferenced. Divide before truncating.
assert.equal(e.big_difference(0xfffffff0,0),1073741820);
assert.equal(e.big_difference(0,0xfffffff0),-1073741820);
"#;
    let mut child = Command::new("node")
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("existing Node required");
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pointer_slot = module
        .functions
        .iter()
        .find(|f| f.name == "pointer_slot")
        .unwrap();
    assert!(
        pointer_slot
            .stack_slots()
            .any(|(_, s)| s.size == 4 && s.align == 4)
    );
    let checksum = module
        .functions
        .iter()
        .find(|f| f.name == "checksum")
        .unwrap();
    assert!(
        checksum
            .stack_slots()
            .any(|(_, s)| s.size == 16 && s.align == 16)
    );
    for target in [Some("wasm32-unknown-unknown"), None] {
        let mut cmd = Command::new("clang");
        if let Some(target) = target {
            cmd.arg(format!("--target={target}"));
        }
        let out = cmd
            .args([
                "-std=gnu11",
                "-ffreestanding",
                "-nostdinc",
                "-Werror",
                "-fsyntax-only",
            ])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/wasm-pointer-data.c"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let native = crate::compile(&source, "pointer.c", crate::Options::default()).unwrap();
    keel_ir::verify_module(&native).unwrap();
}

#[test]
fn storage_data_boundaries_before_pruning() {
    for source in [
        "struct Big { long long big[9223372036854775807LL]; }; int f(void) { return 0; }",
        "union Big { long long big[9223372036854775807LL]; }; int f(void) { return 0; }",
        "struct Big { char big[2147483648ULL][8589934592ULL]; }; int f(void) { return 0; }",
        "struct Big { char a[2147483648ULL], b[2147483648ULL]; }; int f(void) { return 0; }",
        "struct Big { _Alignas(16) char a[4294967295ULL]; }; int f(void) { return 0; }",
        "int f(void) { return sizeof(long long[9223372036854775807LL]); }",
        "static long long big[1][9223372036854775807LL]; int f(void) { return 0; }",
        "static int unused(void) { long long (*p)[1][9223372036854775807LL]; return 0; }",
    ] {
        let error = lower(source, false).unwrap_err();
        assert!(error.message.contains("exceeds wasm32 bounds"), "{error:?}");
    }
    // A layout query at the valid boundary emits no allocation or data object.
    let boundary = lower("struct Big { char a[4294967295ULL]; }; unsigned long f(void) { return sizeof(struct Big); }", false).unwrap();
    assert!(boundary.data.is_empty());
    // Sixteen scaled steps would wrap an unchecked i64 addend back to zero,
    // passing a final-only bounds test. The actual target is one byte, so this
    // witness never needs a large data allocation, even if the gate regresses.
    for step in [" + 1073741824L", " - 1073741824L"] {
        let source = format!(
            "typedef char Row[1073741824]; static char data; static Row *bad = (Row *)&data{}; int f(void) {{ return 0; }}",
            step.repeat(16)
        );
        assert!(lower(&source, false).is_err(), "accepted {source}");
    }
    for src in [
        "extern int missing; int f(void) { return 0; }",
        "static long long big[9223372036854775807LL]; int f(void) { return 0; }",
        "static int unused(void) { long long big[9223372036854775807LL]; return 0; }",
        "typedef char Row[1073741824]; static Row data[1]; static Row *bad = data + 1073741824L + 1073741824L + 1073741824L + 1073741824L + 1073741824L + 1073741824L + 1073741824L + 1073741824L; int f(void) { return 0; }",
        "static int data; static int *bad = &data - 1; int f(void) { return 0; }",
        "static int data; static int *bad = &data + 2; int f(void) { return 0; }",
        "static unsigned long bad = (unsigned long)&bad; int f(void) { return 0; }",
        "static char *bad = \"text\"; int f(void) { return 0; }",
        "static _Thread_local int bad; int f(void) { return 0; }",
        "static int bad __attribute__((weak)); int f(void) { return 0; }",
        "static float bad; int f(void) { return 0; }",
        "static int (*bad)(void); int f(void) { return 0; }",
        "static int unused(void) { struct S { int x; } a, b; a = b; return 0; }",
        "static int unused(void) { struct S { float x; } a; return 0; }",
        "static int unused(void) { (void)(float *)0; return 0; }",
        "static int unused(void) { int a[17] = {1}; return 0; }",
        "static int unused(void) { char a[8] = \"test\"; return 0; }",
        "static int *p = (int[]){1}; int f(void) { return 0; }",
        "int *f(void);",
        "int f(int *p);",
    ] {
        assert!(lower(src, false).is_err(), "accepted {src}");
    }
    let module = lower("int target[2]; struct R { int a; int *p; int b; int *q; }; struct R r = {0x11223344, target + 1, 0x55667788, target + 2}; unsigned long g = (unsigned long)0x100000007ULL; void *n = (void *)0x1234567880000001ULL; char text[4] = \"ab\";", false).unwrap();
    keel_ir::verify_module_with_layout(&module, keel_ir::DataLayout::WASM32).unwrap();
    let record = module.data_object("r").unwrap().init.as_ref().unwrap();
    assert_eq!(record.align, 4);
    assert_eq!(
        record.checked_size_with_layout(keel_ir::DataLayout::WASM32),
        Some(16)
    );
    let mut cursor = 0;
    let mut pointers = Vec::new();
    for item in &record.items {
        if let DataItem::Ptr { symbol, addend } = item {
            pointers.push((cursor, symbol.as_str(), *addend));
        }
        cursor += item.size_with_layout(keel_ir::DataLayout::WASM32);
    }
    assert_eq!(pointers, [(4, "target", 4), (12, "target", 8)]);
    let bytes = |name: &str| {
        let init = module.data_object(name).unwrap().init.as_ref().unwrap();
        let mut out = Vec::new();
        for item in &init.items {
            match item {
                DataItem::Bytes(b) => out.extend_from_slice(b),
                DataItem::Zero(n) => out.resize(out.len() + *n as usize, 0),
                _ => panic!("expected serialized bytes"),
            }
        }
        out
    };
    assert_eq!(bytes("g"), 7u32.to_le_bytes());
    assert_eq!(bytes("n"), 0x80000001u32.to_le_bytes());
    assert_eq!(bytes("text"), b"ab\0\0");
    let native = crate::compile("int target[2]; struct R { int a; int *p; int b; int *q; }; struct R r = {1, target + 1, 2, target + 2};", "data.c", crate::Options::default()).unwrap();
    keel_ir::verify_module(&native).unwrap();
    assert_eq!(
        native
            .data_object("r")
            .unwrap()
            .init
            .as_ref()
            .unwrap()
            .size(),
        32
    );
}

#[test]
fn pointer_widths_wrap_and_exact_host_footprint() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let module = lower(
        r#"
struct R { unsigned int before; void *volatile pointer; volatile long value; unsigned int after; };
static long long helper(long long x) { volatile long long saved = x; return saved; }
unsigned long long store(unsigned int base, unsigned long long bits) {
    struct R *r = (struct R *)(unsigned long)base;
    r->pointer = (void *)bits;
    r->value = (long)r->pointer;
    unsigned long long live = (unsigned long long)r->pointer;
    return live ^ (unsigned long long)helper((long long)r->value);
}
unsigned long long wrap(unsigned int address, long delta) {
    unsigned char *p = (unsigned char *)(unsigned long)address;
    p += delta;
    return (unsigned long long)p;
}
long long signed_bits(unsigned long long bits) { return (long long)(long)(void *)bits; }
unsigned long long unsigned_bits(unsigned long long bits) { return (unsigned long long)(unsigned long)(void *)bits; }
"#,
        false,
    )
    .unwrap();
    let bytes = keel_wasm::emit_wasm32_runtime_module(
        &module,
        &keel_wasm::Wasm32RuntimeOptions {
            memory: keel_wasm::Wasm32MemoryOptions {
                scalar: keel_wasm::Options {
                    exports: ["store", "wrap", "signed_bits", "unsigned_bits"]
                        .into_iter()
                        .map(|name| keel_wasm::Export {
                            name: name.into(),
                            function: name.into(),
                        })
                        .collect(),
                },
                pages: 1,
                export_name: "memory".into(),
            },
            data_floor: 1024,
            stack_bottom: 32768,
            stack_top: 65536,
        },
    )
    .unwrap();
    // Numeric-address arithmetic checks the private IR normalization contract;
    // it does not assert C object provenance for wrapping addresses. Only the
    // small, aligned host record is dereferenced. BigInt supplies the oracle.
    let script = r#"
const assert = require('assert/strict'), bytes = require('fs').readFileSync(0);
assert(WebAssembly.validate(bytes));
const e = new WebAssembly.Instance(new WebAssembly.Module(bytes)).exports;
const memory = new Uint8Array(e.memory.buffer), view = new DataView(e.memory.buffer);
const inputs = [0n, 1n, 0x7fffffffn, 0x80000000n, 0xffffffffn, -1n,
                0x1234567880000001n, -0x1234567880000001n];
for (const bits of inputs) {
 const low = BigInt.asUintN(32,bits), signed = BigInt.asIntN(32,bits);
 assert.equal(e.signed_bits(bits),signed);
 assert.equal(e.unsigned_bits(bits),low);
 memory.fill(0xa7,8192,8224);
 const expected = memory.slice(8192,8224);
 for (let i=0;i<4;i++) expected[8+i]=expected[12+i]=Number((low>>BigInt(8*i))&255n);
 assert.equal(BigInt.asUintN(64,e.store(8196,bits)),BigInt.asUintN(64,low^signed));
 assert.deepEqual(memory.slice(8192,8224),expected);
 assert.equal(view.getUint32(8200,true),Number(low));
}
for (const address of [0,1,0x7fffffff,0x80000000,0xffffffff]) {
 for (const delta of [-2147483648,-257,-1,0,1,257,2147483647]) {
   assert.equal(e.wrap(address,delta),BigInt.asUintN(32,BigInt(address)+BigInt(delta)));
 }
}
"#;
    let mut child = Command::new("node")
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("existing Node required");
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn pointer_decrement_executes_with_canaries_and_extreme_offsets() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    // This last maintained fixture function is self-contained/header-free.
    let source = include_str!("../../tests/fixtures/wasm-pointer-data.c");
    let source = &source[source.find("int decrement(long offset)").unwrap()..];
    let module = lower(source, false).unwrap();
    let bytes = keel_wasm::emit_wasm32_runtime_module(
        &module,
        &keel_wasm::Wasm32RuntimeOptions {
            memory: keel_wasm::Wasm32MemoryOptions {
                scalar: keel_wasm::Options {
                    exports: vec![keel_wasm::Export {
                        name: "decrement".into(),
                        function: "decrement".into(),
                    }],
                },
                pages: 1,
                export_name: "memory".into(),
            },
            data_floor: 1024,
            stack_bottom: 32768,
            stack_top: 65536,
        },
    )
    .unwrap();
    let script = r#"
const assert = require('assert/strict'), bytes = require('fs').readFileSync(0);
assert(WebAssembly.validate(bytes));
const e = new WebAssembly.Instance(new WebAssembly.Module(bytes)).exports;
for (let i=0; i<1000; i++) {
 for (const offset of [-2147483648,-1,0,5,2147483647]) assert.equal(e.decrement(offset),-1);
 for (let offset=1; offset<=4; offset++) {
   const old=[0,22,33,44,90][offset], next=[0,12,23,34,45][offset];
   assert.equal(e.decrement(offset),old*65536+next*256+1);
 }
}
"#;
    let mut child = Command::new("node")
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("existing Node required");
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
