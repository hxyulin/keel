//! Private integer-only lowering checkpoint, not a C ABI or production target.
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
    if e.ty != Ty::Void && !integer(&e.ty) {
        return Err(reject(e.loc, "non-integer values (including pointers)"));
    }
    match e.kind {
        ExprKind::Int(_) | ExprKind::Local(_) => Ok(()),
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

fn statement(s: &Stmt<'_>) -> R<()> {
    match s {
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => expression(e),
        Stmt::Init(_, init, loc) => {
            for item in init.items {
                match item {
                    InitItem::Expr {
                        offset: 0,
                        expr,
                        bits: None,
                    } => expression(expr)?,
                    _ => return Err(reject(*loc, "aggregate/byte/bit-field initializers")),
                }
            }
            Ok(())
        }
        Stmt::Block(ss) | Stmt::Scoped(_, ss) => statements(ss),
        Stmt::If(e, yes, no) => {
            expression(e)?;
            statement(yes)?;
            if let Some(no) = no {
                statement(no)?;
            }
            Ok(())
        }
        Stmt::While(e, body) | Stmt::DoWhile(body, e) => {
            expression(e)?;
            statement(body)
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => {
            if let Some(init) = init {
                statement(init)?;
            }
            if let Some(cond) = cond {
                expression(cond)?;
            }
            if let Some(step) = step {
                expression(step)?;
            }
            statement(body)
        }
        Stmt::Switch { cond, body, .. } => {
            expression(cond)?;
            statement(body)
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

fn statements(ss: &[Stmt<'_>]) -> R<()> {
    for s in ss {
        statement(s)?;
    }
    Ok(())
}

impl Gen<'_> {
    pub(super) fn check_private_scalar_profile(&self) -> R<()> {
        if self.debug {
            return Err(reject(Loc::default(), "debug output"));
        }
        for g in &self.p.globals {
            let Ty::Func(fty) = g.ty else {
                return Err(reject(
                    Loc::default(),
                    "static/external data and symbolic initializers",
                ));
            };
            signature(fty, Loc::default())?;
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
                if !integer(&local.ty) || local.cleanup.is_some() {
                    return Err(reject(
                        def.loc,
                        "non-integer/atomic/aggregate locals or cleanup",
                    ));
                }
                if local.align > 16 {
                    return Err(reject(def.loc, "local alignment above 16"));
                }
            }
            statements(def.body)?;
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
    Gen::new(&p, funcs, &files, debug).run()
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
        "static int unused(void) { int *p = 0; return 0; }",
        "static int unused(void) { _Atomic int x; return 0; }",
        "static int unused(int n) { int a[n]; return 0; }",
        "static int unused(void) { int x __attribute__((aligned(32))); return 0; }",
        "static int unused(void) { return __builtin_popcount(1); }",
        "static int unused(void) { __asm__(\"\"); return 0; }",
        "int f(double x);",
        "int f(__int128 x);",
        "static int unused(void) { int a[4]; return 0; }",
        "static int unused(void) { struct S { int x; } s; return 0; }",
        "static int unused(void) { return __builtin_frame_address(0) != 0; }",
        "static int unused(void) { if (0) return __builtin_popcount(1); return 0; }",
        "int f(char x);",
        "int f(int x, ...);",
        "int x;",
        "int f(void) { return (int)(long *)1; }",
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

