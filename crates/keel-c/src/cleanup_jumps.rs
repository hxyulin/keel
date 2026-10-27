//! Declaration-lifetime regions for cleanup-aware C jumps.

use crate::Error;
use crate::ast::{Expr, ExprKind, FuncDef, Init, InitItem, LocalId, Stmt};
use keel_ir::fxhash::FxHashMap as HashMap;

#[derive(Clone, Default)]
struct Region {
    cleanups: Vec<LocalId>,
    statement_expressions: Vec<usize>,
}

/// Labels carry declaration regions, not just enclosing compound scopes.
#[derive(Default)]
pub(crate) struct CleanupJumps<'a> {
    labels: HashMap<&'a str, Region>,
}

#[derive(Clone, Copy)]
enum JumpKind {
    Direct,
    Computed,
    Asm,
}

struct Edge<'a> {
    source: Region,
    target: &'a str,
    kind: JumpKind,
}

struct Scan<'a, 'f> {
    def: &'f FuncDef<'a>,
    jumps: CleanupJumps<'a>,
    current: Region,
    edges: Vec<Edge<'a>>,
}

impl<'a> CleanupJumps<'a> {
    pub(crate) fn analyze(def: &FuncDef<'a>) -> Result<Self, Error> {
        if !def.locals.iter().any(|local| local.cleanup.is_some()) {
            return Ok(Self::default());
        }
        let mut scan = Scan {
            def,
            jumps: Self::default(),
            current: Region::default(),
            edges: Vec::new(),
        };
        scan.statements(def.body);
        if def.has_vm && !scan.edges.is_empty() {
            return Err(Error::new(
                def.loc,
                "goto in a function containing variably modified types (VLAs) is not supported yet",
            ));
        }
        for edge in &scan.edges {
            let Some(target) = scan.jumps.labels.get(edge.target) else {
                // Undefined labels retain the lowering diagnostic.
                continue;
            };
            if !edge.source.cleanups.starts_with(&target.cleanups) {
                return Err(Error::new(
                    def.loc,
                    "goto bypasses initialization of a cleanup local",
                ));
            }
            if !edge
                .source
                .statement_expressions
                .starts_with(&target.statement_expressions)
            {
                return Err(Error::new(def.loc, "goto enters a statement expression"));
            }
            // Clang rejects dynamic indirect and asm edges that bypass either
            // initialization or teardown. A literal &&label is a direct edge.
            if !matches!(edge.kind, JumpKind::Direct)
                && (edge.source.cleanups != target.cleanups
                    || edge.source.statement_expressions != target.statement_expressions)
            {
                return Err(Error::new(
                    def.loc,
                    match edge.kind {
                        JumpKind::Computed => {
                            "computed goto across cleanup or statement-expression regions is not supported"
                        }
                        JumpKind::Asm => {
                            "asm goto across cleanup or statement-expression regions is not supported"
                        }
                        JumpKind::Direct => {
                            unreachable!("direct jumps passed indirect-edge validation")
                        }
                    },
                ));
            }
        }
        Ok(scan.jumps)
    }

    pub(crate) fn depth(&self, label: &str) -> usize {
        self.labels
            .get(label)
            .map_or(0, |region| region.cleanups.len())
    }
}

impl<'a> Scan<'a, '_> {
    fn edge(&mut self, target: &'a str, kind: JumpKind) {
        self.edges.push(Edge {
            source: self.current.clone(),
            target,
            kind,
        });
    }

    fn statements(&mut self, statements: &[Stmt<'a>]) {
        for stmt in statements {
            self.statement(stmt);
        }
    }

    fn init(&mut self, init: &Init<'a>) {
        for item in init.items {
            if let InitItem::Expr { expr, .. } = item {
                self.expression(expr);
            }
        }
    }

    fn statement(&mut self, stmt: &Stmt<'a>) {
        match stmt {
            Stmt::Init(id, init, _) => {
                self.init(init);
                if self.def.locals[*id].cleanup.is_some() {
                    self.current.cleanups.push(*id);
                }
            }
            Stmt::Declare(id) => {
                if self.def.locals[*id].cleanup.is_some() {
                    self.current.cleanups.push(*id);
                }
            }
            Stmt::Label(label) => {
                self.jumps.labels.insert(label, self.current.clone());
            }
            Stmt::Goto(label) => self.edge(label, JumpKind::Direct),
            Stmt::GotoIndirect(expr) => {
                self.expression(expr);
                if let ExprKind::LabelAddr(label) = expr.kind {
                    self.edge(self.def.label_addrs[label - 1], JumpKind::Direct);
                } else {
                    for label in &self.def.label_addrs {
                        self.edge(label, JumpKind::Computed);
                    }
                }
            }
            Stmt::Asm(asm) => {
                for (expr, _) in asm.outputs {
                    self.expression(expr);
                }
                for expr in asm.inputs {
                    self.expression(expr);
                }
                for label in asm.goto_labels {
                    self.edge(label, JumpKind::Asm);
                }
            }
            Stmt::Expr(expr) | Stmt::Return(Some(expr), _) => self.expression(expr),
            Stmt::Block(statements) => self.statements(statements),
            Stmt::Scoped(_, statements) => {
                let depth = self.current.cleanups.len();
                self.statements(statements);
                self.current.cleanups.truncate(depth);
            }
            Stmt::If(expr, yes, no) => {
                self.expression(expr);
                self.statement(yes);
                if let Some(no) = no {
                    self.statement(no);
                }
            }
            Stmt::While(expr, body) => {
                self.expression(expr);
                self.statement(body);
            }
            Stmt::DoWhile(body, expr) => {
                self.statement(body);
                self.expression(expr);
            }
            Stmt::Switch { cond, body, .. } => {
                self.expression(cond);
                self.statement(body);
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                if let Some(init) = init {
                    self.statement(init);
                }
                if let Some(cond) = cond {
                    self.expression(cond);
                }
                self.statement(body);
                if let Some(step) = step {
                    self.expression(step);
                }
            }
            Stmt::Case(_) | Stmt::Break | Stmt::Continue | Stmt::Return(None, _) => {}
        }
    }

    fn expression(&mut self, expr: &Expr<'a>) {
        match &expr.kind {
            ExprKind::StmtExpr {
                scope,
                stmts,
                value,
            } => {
                let depth = self.current.cleanups.len();
                self.current.statement_expressions.push(*scope);
                self.statements(stmts);
                if let Some(value) = value {
                    self.expression(value);
                }
                self.current.cleanups.truncate(depth);
                self.current.statement_expressions.pop();
            }
            ExprKind::Unary(_, a)
            | ExprKind::Cast(a)
            | ExprKind::UnionCast(a, _)
            | ExprKind::Deref(a)
            | ExprKind::VaStart(a)
            | ExprKind::VaArg(a)
            | ExprKind::IncDec { lhs: a, .. }
            | ExprKind::Addr(a)
            | ExprKind::Member { base: a, .. } => self.expression(a),
            ExprKind::VectorIndex(a, b)
            | ExprKind::Binary(_, a, b)
            | ExprKind::PtrAdd(a, b, _)
            | ExprKind::PtrDiff(a, b, _)
            | ExprKind::LogAnd(a, b)
            | ExprKind::LogOr(a, b)
            | ExprKind::Assign(a, b)
            | ExprKind::Comma(a, b)
            | ExprKind::OpAssign { lhs: a, rhs: b, .. }
            | ExprKind::PtrOpAssign { lhs: a, rhs: b, .. } => {
                self.expression(a);
                self.expression(b);
            }
            ExprKind::Cond(a, b, c) => {
                self.expression(a);
                self.expression(b);
                self.expression(c);
            }
            ExprKind::Call(callee, args) => {
                self.expression(callee);
                for arg in *args {
                    self.expression(arg);
                }
            }
            ExprKind::Builtin(_, args) => {
                for arg in *args {
                    self.expression(arg);
                }
            }
            ExprKind::CompoundLit(_, init) => self.init(init),
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::LabelAddr(_)
            | ExprKind::Local(_)
            | ExprKind::Global(_) => {}
        }
    }
}
