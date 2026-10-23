//! The checked GNU C inline-assembly subset. Assembly operands remain typed
//! expressions until lowering captures their locations/values exactly once.

use super::*;

impl<'a> Parser<'a> {
    /// Adjacent narrow strings, without a C terminator (the lexer stores none).
    fn asm_string(&mut self) -> R<String> {
        let loc = self.loc();
        let mut text = Vec::new();
        let mut any = false;
        while let Tok::Str { bytes, wide, .. } = self.peek() {
            if wide {
                return Err(Error::new(loc, "asm requires narrow string literals"));
            }
            any = true;
            text.extend_from_slice(bytes);
            self.advance();
        }
        if !any || text.contains(&0) {
            return Err(Error::new(
                loc,
                "asm requires a string literal without embedded NUL",
            ));
        }
        String::from_utf8(text).map_err(|_| Error::new(loc, "asm requires UTF-8 text"))
    }

    pub(super) fn asm_statement(&mut self) -> R<Stmt<'a>> {
        use keel_ir::InlineAsm;
        let loc = self.loc();
        self.advance();
        while matches!(
            self.peek(),
            Tok::Ident(
                "volatile"
                    | "__volatile"
                    | "__volatile__"
                    | "inline"
                    | "__inline"
                    | "__inline__"
                    | "goto"
            )
        ) {
            if self.is("goto") {
                return Err(Error::new(loc, "asm goto is not supported"));
            }
            // All IR asm is pinned. `inline` is only an inlining size hint.
            self.advance();
        }
        if self.target != Target::Aarch64AppleDarwin {
            return Err(Error::new(
                loc,
                "C inline assembly requires the AArch64 backend; x86 inline assembly is not supported",
            ));
        }
        self.expect("(")?;
        let template = self.asm_string()?;
        let mut outputs = Vec::new();
        let mut inputs = Vec::new();
        let mut constraints: Vec<String> = Vec::new();
        let mut names = Vec::new();
        let extended = self.eat(":");
        if extended {
            // Outputs, then explicit inputs; hidden read/write inputs go last.
            for section in 0..2 {
                if !self.is(":") && !self.is(")") {
                    loop {
                        let name = if self.eat("[") {
                            let name = self.ident()?;
                            self.expect("]")?;
                            if names.iter().any(|(n, _)| *n == name) {
                                return Err(Error::new(loc, "duplicate asm operand name"));
                            }
                            Some(name)
                        } else {
                            None
                        };
                        let mut constraint = self.asm_string()?;
                        if let Some(name) = name {
                            names.push((name, outputs.len() + inputs.len()));
                        }
                        self.expect("(")?;
                        let mut expr = self.expr()?;
                        self.expect(")")?;
                        if section == 0 {
                            let readwrite = constraint.starts_with('+');
                            if !readwrite && !constraint.starts_with('=') {
                                return Err(Error::new(
                                    loc,
                                    "asm output constraint must start with `=` or `+`",
                                ));
                            }
                            let code = &constraint[1..];
                            let code = code.strip_prefix('&').unwrap_or(code);
                            if !matches!(code, "r" | "w") {
                                return Err(Error::new(
                                    loc,
                                    "unsupported asm output constraint (supported: =r, +r, =w, +w, with optional &; memory outputs are not supported)",
                                ));
                            }
                            self.check_lvalue(&expr)?;
                            if asm_const(expr.sugar, &self.typedefs)
                                || matches!(expr.ty, Ty::Atomic(_))
                            {
                                return Err(Error::new(
                                    expr.loc,
                                    "asm output requires a modifiable non-atomic lvalue",
                                ));
                            }
                            if matches!(
                                expr.kind,
                                ExprKind::Member { bits: Some(_), .. } | ExprKind::VectorIndex(..)
                            ) {
                                return Err(Error::new(
                                    expr.loc,
                                    "asm bit-field and vector-element outputs are not supported",
                                ));
                            }
                            asm_register_type(code, expr.ty, loc)?;
                            constraint = format!("={code}");
                            outputs.push((expr, readwrite));
                        } else {
                            if let Some(name) = constraint
                                .strip_prefix('[')
                                .and_then(|s| s.strip_suffix(']'))
                            {
                                let n = names
                                    .iter()
                                    .find(|(n, _)| *n == name)
                                    .map(|(_, i)| *i)
                                    .filter(|i| *i < outputs.len())
                                    .ok_or_else(|| {
                                        Error::new(
                                            loc,
                                            "asm matching constraint must name an output",
                                        )
                                    })?;
                                constraint = n.to_string();
                            }
                            match constraint.as_str() {
                                "m" => {
                                    if matches!(expr.ty, Ty::Void | Ty::Func(_)) {
                                        return Err(Error::new(
                                            expr.loc,
                                            "asm memory input requires an object lvalue",
                                        ));
                                    }
                                    // This both checks addressability and marks locals as memory.
                                    expr = self.address_of(expr, loc)?;
                                }
                                "i" | "n" => {
                                    let value = crate::eval::eval_constant(&expr, false)
                                        .filter(|_| expr.ty.is_integer());
                                    let Some(crate::eval::Const::Int(value)) = value else {
                                        return Err(Error::new(
                                            expr.loc,
                                            "asm immediate requires an integer constant expression",
                                        ));
                                    };
                                    if expr.ty.ir() == keel_ir::Type::I128 {
                                        return Err(Error::new(
                                            expr.loc,
                                            "128-bit asm immediates are not supported",
                                        ));
                                    }
                                    expr.kind = ExprKind::Int(value);
                                }
                                "r" | "w" => {
                                    expr = self.rvalue(expr);
                                    asm_register_type(&constraint, expr.ty, loc)?;
                                }
                                _ => {
                                    let n = constraint.parse::<usize>().ok().filter(|n| *n < outputs.len())
                                        .ok_or_else(|| Error::new(loc, format!("unsupported asm input constraint `{constraint}` (supported: r, w, m, i, n, output number or [name])")))?;
                                    expr = self.rvalue(expr);
                                    asm_register_type(
                                        constraints[n].trim_start_matches('='),
                                        expr.ty,
                                        expr.loc,
                                    )?;
                                    if expr.ty.ir() != outputs[n].0.ty.ir() {
                                        return Err(Error::new(
                                            expr.loc,
                                            "asm matching input and output must have the same IR type",
                                        ));
                                    }
                                    constraint = n.to_string();
                                }
                            }
                            inputs.push(expr);
                        }
                        constraints.push(constraint);
                        if !self.eat(",") {
                            break;
                        }
                    }
                }
                if section == 0 && !self.eat(":") {
                    break;
                }
            }
            let mut tied = HashSet::default();
            for c in constraints.iter().skip(outputs.len()) {
                if let Ok(n) = c.parse::<usize>()
                    && (outputs[n].1 || !tied.insert(n))
                {
                    return Err(Error::new(loc, "asm output is tied to more than one input"));
                }
            }
            for (n, (_, readwrite)) in outputs.iter().enumerate() {
                if *readwrite {
                    constraints.push(n.to_string());
                }
            }
            if self.eat(":") {
                if !self.is(")") && !self.is(":") {
                    loop {
                        let clobber = self.asm_string()?;
                        if !matches!(clobber.as_str(), "memory" | "cc") {
                            let reg = keel_ir::PhysReg::parse(&clobber)
                                .map_err(|e| Error::new(loc, e))?;
                            if !reg.float && matches!(reg.num, 18 | 29) {
                                return Err(Error::new(
                                    loc,
                                    "asm cannot clobber reserved x18 or frame pointer x29",
                                ));
                            }
                        }
                        constraints.push(format!("~{{{clobber}}}"));
                        if !self.eat(",") {
                            break;
                        }
                    }
                }
                if self.is(":") {
                    return Err(Error::new(loc, "asm goto label operands are not supported"));
                }
            }
        } else if !template.trim().is_empty() && template.trim() != "nop" {
            return Err(Error::new(
                loc,
                "basic asm currently supports only an empty template or `nop`; use extended asm with explicit operands and clobbers",
            ));
        }
        self.expect(")")?;
        self.expect(";")?;
        let template = if extended {
            asm_template(&template, &names, outputs.len() + inputs.len(), loc)?
        } else {
            template
        };
        let constraints = constraints.join(",");
        let types: Vec<_> = outputs.iter().map(|(e, _)| e.ty.ir()).collect();
        InlineAsm::new(&template, &constraints, &types).map_err(|e| Error::new(loc, e))?;
        Ok(Stmt::Asm(self.nodes.alloc(AsmStmt {
            template: self.text.intern(&template),
            constraints: self.text.intern(&constraints),
            outputs: self.slice(&outputs),
            inputs: self.slice(&inputs),
            loc,
        })))
    }
}

/// Keep named substitutions separate from token parsing: %% never starts a name.
fn asm_template(template: &str, names: &[(&str, usize)], count: usize, loc: Loc) -> R<String> {
    let mut chars = template.chars().peekable();
    let mut text = String::new();
    while let Some(c) = chars.next() {
        text.push(c);
        if c != '%' {
            continue;
        }
        if chars.next_if_eq(&'%').is_some() {
            text.push('%');
            continue;
        }
        if let Some(m) = chars.next_if(|c| c.is_ascii_alphabetic()) {
            if !"wxsdqhbc".contains(m) {
                return Err(Error::new(
                    loc,
                    format!("unsupported asm template modifier `%{m}`"),
                ));
            }
            text.push(m);
        }
        let n = if chars.next_if_eq(&'[').is_some() {
            let mut name = String::new();
            while let Some(c) = chars.next_if(|c| *c != ']') {
                name.push(c);
            }
            if chars.next() != Some(']') {
                return Err(Error::new(loc, "unclosed asm operand name"));
            }
            names
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, n)| *n)
                .ok_or_else(|| Error::new(loc, format!("unknown asm operand name `{name}`")))?
        } else {
            let mut digits = String::new();
            while let Some(c) = chars.next_if(char::is_ascii_digit) {
                digits.push(c);
            }
            digits
                .parse::<usize>()
                .map_err(|_| Error::new(loc, "asm `%` requires an operand number/name or `%%`"))?
        };
        if n >= count {
            return Err(Error::new(
                loc,
                "asm template operand number is out of range",
            ));
        }
        // A named reference has an explicit end. Preserve it if a literal
        // digit follows: %[in]0 must not become operand %10.
        if chars.peek().is_some_and(char::is_ascii_digit) {
            text.push('[');
            text.push_str(&n.to_string());
            text.push(']');
        } else {
            text.push_str(&n.to_string());
        }
    }
    Ok(text)
}

fn asm_register_type(code: &str, ty: Ty<'_>, loc: Loc) -> R<()> {
    let ok = match code {
        "r" => matches!(ty, Ty::Int(k) if k.size() <= 8) || ty.is_ptr(),
        "w" => matches!(ty, Ty::Float | Ty::Double),
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(Error::new(
            loc,
            format!("asm `{code}` constraint does not support this operand type"),
        ))
    }
}

fn asm_const(sugar: Sugar<'_>, defs: &[TypedefDef<'_>]) -> bool {
    match sugar {
        Sugar::Const(_) => true,
        Sugar::Typedef(i) => asm_const(defs[i].sugar, defs),
        Sugar::Volatile(s) | Sugar::Restrict(s) => asm_const(*s, defs),
        _ => false,
    }
}
