//! The checked GNU C inline-assembly subset. Assembly operands remain typed
//! expressions until lowering captures their locations/values exactly once.

use super::*;

impl<'a> Parser<'a> {
    /// Adjacent narrow strings, without a C terminator (the lexer stores none).
    pub(super) fn asm_string(&mut self) -> R<String> {
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

    pub(super) fn asm_declaration(&mut self) -> R<()> {
        let loc = self.loc();
        if self.target != Target::X86_64LinuxGnu {
            return Err(Error::new(loc, "file-scope assembly requires x86-64 Linux"));
        }
        self.advance();
        self.expect("(")?;
        let text = self.asm_string()?;
        self.expect(")")?;
        self.expect(";")?;
        self.assembly.push(text);
        Ok(())
    }

    pub(super) fn asm_statement(&mut self) -> R<Stmt<'a>> {
        use keel_ir::InlineAsm;
        let loc = self.loc();
        self.advance();
        let mut goto = false;
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
                goto = true;
            }
            // All IR asm is pinned. `inline` is only an inlining size hint.
            self.advance();
        }
        let x86 = self.target == Target::X86_64LinuxGnu;
        if !x86 && self.target != Target::Aarch64AppleDarwin {
            return Err(Error::new(
                loc,
                "C inline assembly requires the AArch64 Darwin or x86-64 Linux backend",
            ));
        }
        if goto && !x86 {
            return Err(Error::new(loc, "asm goto requires x86-64 Linux"));
        }
        if goto {
            self.fs
                .as_mut()
                .unwrap()
                .cleanup_unsupported_exit
                .get_or_insert((loc, "asm goto"));
        }
        self.expect("(")?;
        let template = self.asm_string()?;
        let mut goto_labels = Vec::new();
        let mut outputs = Vec::new();
        let mut output_memory = Vec::new();
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
                            if matches!(expr.kind, ExprKind::Builtin(Builtin::StackPointer, _)) {
                                return Err(Error::new(
                                    expr.loc,
                                    "stack-pointer asm outputs require a stack-call contract",
                                ));
                            }
                            let readwrite = constraint.starts_with('+');
                            if !readwrite && !constraint.starts_with('=') {
                                return Err(Error::new(
                                    loc,
                                    "asm output constraint must start with `=` or `+`",
                                ));
                            }
                            let code = &constraint[1..];
                            let code = code.strip_prefix('&').unwrap_or(code);
                            let flag = x86 && code.starts_with("@cc");
                            if flag && code.contains(',') {
                                return Err(Error::new(
                                    loc,
                                    "flag output constraints do not allow alternatives",
                                ));
                            }
                            if flag && (readwrite || constraint.starts_with("=&")) {
                                return Err(Error::new(
                                    loc,
                                    "flag outputs require = without + or &",
                                ));
                            }
                            let code = if x86 && matches!(code, "rm" | "mr" | "qm" | "mq" | "g") {
                                if !asm_gpr_type(expr.ty) {
                                    return Err(Error::new(
                                        loc,
                                        "asm output choices require a scalar integer or pointer; use m for other objects",
                                    ));
                                }
                                "r"
                            } else {
                                code
                            };
                            if !(matches!(code, "r" | "w") && (!x86 || code == "r")
                                || flag
                                || x86
                                    && matches!(
                                        code,
                                        "q" | "a" | "b" | "c" | "d" | "S" | "D" | "m"
                                    ))
                            {
                                return Err(Error::new(
                                    loc,
                                    if x86 {
                                        format!(
                                            "unsupported asm output constraint `{constraint}` (supported: r, q, a, b, c, d, S, D, m, rm, g and =@cc<condition>; & applies to register outputs)"
                                        )
                                    } else {
                                        "unsupported asm output constraint (supported: =r, +r, =w, +w, with optional &; memory outputs are not supported)".into()
                                    },
                                ));
                            }
                            if !(code == "m" && matches!(expr.ty, Ty::Array(..))) {
                                self.check_lvalue(&expr)?;
                            }
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
                            let memory = code == "m";
                            if memory {
                                if constraint[1..].starts_with('&') {
                                    return Err(Error::new(
                                        loc,
                                        "early-clobber requires a register output",
                                    ));
                                }
                                // Capture the object's address, never a scalar copy of its contents.
                                expr = self.address_of(expr, loc)?;
                                constraint = if readwrite { "mreadwrite" } else { "mwrite" }.into();
                            } else {
                                if flag {
                                    if !matches!(expr.ty, Ty::Int(k) if k.size() <= 8) {
                                        return Err(Error::new(
                                            loc,
                                            "asm flag output requires an integer through 64 bits",
                                        ));
                                    }
                                } else {
                                    asm_register_type(code, expr.ty, loc)?;
                                }
                                let early = if constraint[1..].starts_with('&') {
                                    "&"
                                } else {
                                    ""
                                };
                                constraint = format!("={early}{}", asm_constraint(code, x86));
                            }
                            outputs.push((expr, readwrite));
                            output_memory.push(memory);
                        } else {
                            if x86 && matches!(constraint.as_str(), "er" | "re" | "Ir" | "rI") {
                                constraint =
                                    if constraint.contains('e') { "re" } else { "rI" }.into();
                            }
                            if x86 && constraint == "dN" {
                                constraint = "Nd".into();
                            }
                            if x86 && let Some((immediate, memory)) = asm_choice(&constraint) {
                                constraint = if asm_gpr_type(expr.ty) {
                                    if immediate { "ri" } else { "r" }
                                } else if memory {
                                    "m"
                                } else {
                                    return Err(Error::new(
                                        loc,
                                        "asm register choice requires a scalar integer or pointer",
                                    ));
                                }
                                .into();
                            }
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
                                "ri" | "re" | "rI" | "Nd" if x86 => {
                                    expr = self.rvalue(expr);
                                    asm_register_type("r", expr.ty, loc)?;
                                }
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
                                    if x86 {
                                        expr = self.rvalue(expr);
                                        if !expr.ty.is_integer()
                                            && expr.ty.ir() != keel_ir::Type::Ptr
                                        {
                                            return Err(Error::new(
                                                expr.loc,
                                                "asm immediate requires an integer or pointer",
                                            ));
                                        }
                                    } else {
                                        let value = crate::eval::eval_constant(&expr, false)
                                            .filter(|_| expr.ty.is_integer());
                                        let Some(crate::eval::Const::Int(value)) = value else {
                                            return Err(Error::new(
                                                expr.loc,
                                                "asm immediate requires an integer constant expression",
                                            ));
                                        };
                                        expr.kind = ExprKind::Int(value);
                                    }
                                    if expr.ty.ir() == keel_ir::Type::I128 {
                                        return Err(Error::new(
                                            expr.loc,
                                            "128-bit asm immediates are not supported",
                                        ));
                                    }
                                }
                                "r" | "w" if !x86 || constraint == "r" => {
                                    expr = self.rvalue(expr);
                                    asm_register_type(&constraint, expr.ty, loc)?;
                                }
                                "q" | "a" | "b" | "c" | "d" | "S" | "D" if x86 => {
                                    expr = self.rvalue(expr);
                                    asm_register_type(&constraint, expr.ty, loc)?;
                                    constraint = asm_constraint(&constraint, true).to_string();
                                }
                                _ => {
                                    let n = constraint.parse::<usize>().ok().filter(|n| *n < outputs.len())
                                        .ok_or_else(|| Error::new(loc, format!("unsupported asm input constraint `{constraint}` (supported: r, q (x86), w (AArch64), m, i, n, x86 Nd and r/q choices with i/m or g, output number or [name])")))?;
                                    if output_memory[n] || constraints[n].starts_with("=@cc") {
                                        return Err(Error::new(
                                            loc,
                                            "asm matching input requires a register output",
                                        ));
                                    }
                                    expr = self.rvalue(expr);
                                    asm_register_type(
                                        constraints[n]
                                            .trim_start_matches('=')
                                            .trim_start_matches('&'),
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
                if *readwrite && !output_memory[n] {
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
                            if (reg.arch == keel_ir::AsmArch::X86) != x86 {
                                return Err(Error::new(
                                    loc,
                                    "asm clobber register belongs to another architecture",
                                ));
                            }
                            if x86 && matches!(reg.num, 4 | 5) {
                                return Err(Error::new(
                                    loc,
                                    "asm cannot clobber the stack or frame pointer",
                                ));
                            }
                            if !x86 && !reg.float && matches!(reg.num, 18 | 29) {
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
                if self.eat(":") {
                    if !goto {
                        return Err(Error::new(
                            loc,
                            "asm goto label operands require the goto qualifier",
                        ));
                    }
                    if !self.is(")") {
                        loop {
                            let label = self.ident()?;
                            if goto_labels.contains(&label)
                                || names.iter().any(|(name, _)| *name == label)
                            {
                                return Err(Error::new(loc, "duplicate asm operand name"));
                            }
                            goto_labels.push(label);
                            if !self.eat(",") {
                                break;
                            }
                        }
                    }
                }
            }
        } else if !template.trim().is_empty() {
            if !x86 && template.trim() != "nop" {
                return Err(Error::new(
                    loc,
                    "basic asm currently supports only an empty template or `nop`; use extended asm with explicit operands and clobbers",
                ));
            }
            if x86 {
                // GNU basic asm preserves GPRs but can access global memory.
                // Model flags conservatively as well; asm is already pinned.
                constraints.extend(["~{memory}".into(), "~{cc}".into()]);
            }
        }
        self.expect(")")?;
        self.expect(";")?;
        if goto && goto_labels.is_empty() {
            return Err(Error::new(
                loc,
                "asm goto requires at least one label operand",
            ));
        }
        if goto && !outputs.is_empty() {
            return Err(Error::new(
                loc,
                "asm goto output operands are not supported",
            ));
        }
        // Register results precede address operands in IR. Preserve GNU's
        // source numbering by remapping only the output references and ties.
        let n_reg = output_memory.iter().filter(|&&m| !m).count();
        let mut next_reg = 0;
        let mut next_mem = n_reg;
        let mut numbering = Vec::new();
        for &memory in &output_memory {
            let next = if memory { &mut next_mem } else { &mut next_reg };
            numbering.push(*next);
            *next += 1;
        }
        numbering.extend(outputs.len()..outputs.len() + inputs.len());
        let mut normalized = Vec::new();
        for memory in [false, true] {
            for (n, &is_memory) in output_memory.iter().enumerate() {
                if memory == is_memory {
                    normalized.push(constraints[n].clone());
                }
            }
        }
        for c in constraints.iter().skip(outputs.len()) {
            normalized.push(
                c.parse::<usize>()
                    .map_or_else(|_| c.clone(), |n| numbering[n].to_string()),
            );
        }
        let mut template = if extended {
            let flags: Vec<_> = constraints
                .iter()
                .take(outputs.len())
                .map(|c| c.starts_with("=@cc"))
                .collect();
            asm_template(
                &template,
                &names,
                &numbering,
                &flags,
                &goto_labels,
                loc,
                x86,
            )?
        } else {
            // Basic asm has no template substitutions. Escape literal percent
            // signs for the IR's extended-template representation.
            template.replace('%', "%%")
        };
        if goto {
            // All outgoing paths converge inside the template before the
            // backend restores GPR homes. Only then does ordinary IR branch.
            normalized.insert(0, "=&r".into());
            template.push_str("\n\tmovl $0, %k0\n\tjmp .Lkeel_asm_goto_%=_end\n");
            for i in 0..goto_labels.len() {
                template.push_str(&format!(
                    ".Lkeel_asm_goto_%=_{i}:\n\tmovl ${}, %k0\n\tjmp .Lkeel_asm_goto_%=_end\n",
                    i + 1
                ));
            }
            template.push_str(".Lkeel_asm_goto_%=_end:\n");
        }
        let constraints = normalized.join(",");
        let mut types: Vec<_> = outputs
            .iter()
            .zip(&output_memory)
            .filter(|(_, m)| !**m)
            .map(|((e, _), _)| e.ty.ir())
            .collect();
        if goto {
            types.insert(0, keel_ir::Type::I32);
        }
        InlineAsm::new(&template, &constraints, &types).map_err(|e| Error::new(loc, e))?;
        Ok(Stmt::Asm(self.nodes.alloc(AsmStmt {
            template: self.text.intern(&template),
            constraints: self.text.intern(&constraints),
            outputs: self.slice(&outputs),
            output_memory: self.slice(&output_memory),
            inputs: self.slice(&inputs),
            goto_labels: self.slice(&goto_labels),
            loc,
        })))
    }
}

/// Keep named substitutions separate from token parsing: %% never starts a name.
fn asm_template(
    template: &str,
    names: &[(&str, usize)],
    numbering: &[usize],
    flags: &[bool],
    goto_labels: &[&str],
    loc: Loc,
    x86: bool,
) -> R<String> {
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
        if x86 && chars.next_if_eq(&'=').is_some() {
            text.push('=');
            continue;
        }
        if !goto_labels.is_empty() && chars.next_if_eq(&'l').is_some() {
            // GNU label numbering counts source operands, before our hidden
            // selector is added. Named labels live in a separate namespace.
            let i = if chars.next_if_eq(&'[').is_some() {
                let name: String = std::iter::from_fn(|| chars.next_if(|c| *c != ']')).collect();
                if chars.next() != Some(']') {
                    return Err(Error::new(loc, "unclosed asm label name"));
                }
                goto_labels
                    .iter()
                    .position(|label| *label == name)
                    .ok_or_else(|| Error::new(loc, format!("unknown asm label operand `{name}`")))?
            } else {
                let digits: String =
                    std::iter::from_fn(|| chars.next_if(char::is_ascii_digit)).collect();
                digits
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| n.checked_sub(numbering.len()))
                    .filter(|&i| i < goto_labels.len())
                    .ok_or_else(|| Error::new(loc, "asm label operand number is out of range"))?
            };
            text.pop(); // The label is assembly text, not an IR operand.
            text.push_str(&format!(".Lkeel_asm_goto_%=_{i}"));
            continue;
        }
        if let Some(m) = chars.next_if(|c| c.is_ascii_alphabetic()) {
            if !(if x86 { "bwkqcan" } else { "wxsdqhbc" }).contains(m) {
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
        if n >= numbering.len() {
            return Err(Error::new(
                loc,
                "asm template operand number is out of range",
            ));
        }
        if flags.get(n) == Some(&true) {
            return Err(Error::new(
                loc,
                "flag outputs cannot be referenced in an asm template",
            ));
        }
        let n = numbering[n] + usize::from(!goto_labels.is_empty());
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
        "r" | "q" | "a" | "b" | "c" | "d" | "S" | "D" | "{rax}" | "{rbx}" | "{rcx}" | "{rdx}"
        | "{rsi}" | "{rdi}" => matches!(ty, Ty::Int(k) if k.size() <= 8) || ty.is_ptr(),
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

fn asm_gpr_type(ty: Ty<'_>) -> bool {
    matches!(ty, Ty::Int(k) if k.size() <= 8) || ty.is_ptr()
}

/// Single-column unions of the already supported x86 classes. Memory is
/// selected for non-GPR objects; scalar choices retain an immediate option.
fn asm_choice(code: &str) -> Option<(bool, bool)> {
    if code == "g" {
        return Some((true, true));
    }
    if code.len() < 2 || !code.contains(['r', 'q']) {
        return None;
    }
    let mut seen = 0u8;
    for c in code.bytes() {
        let bit = match c {
            b'r' => 1,
            b'q' => 2,
            b'i' => 4,
            b'm' => 8,
            _ => return None,
        };
        if seen & bit != 0 {
            return None;
        }
        seen |= bit;
    }
    Some((seen & 4 != 0, seen & 8 != 0))
}

fn asm_const(sugar: Sugar<'_>, defs: &[TypedefDef<'_>]) -> bool {
    match sugar {
        Sugar::Const(_) => true,
        Sugar::Typedef(i) => asm_const(defs[i].sugar, defs),
        Sugar::Volatile(s) | Sugar::Restrict(s) | Sugar::Array(s) => asm_const(*s, defs),
        _ => false,
    }
}

/// GNU register constraints become shared classes or target-tagged identities.
fn asm_constraint(code: &str, x86: bool) -> &str {
    if !x86 {
        return code;
    }
    match code {
        // In x86-64, q admits all integer registers, just as r does. This is
        // not the four-register q class used in 32-bit mode, nor uppercase Q.
        "q" => "r",
        "a" => "{rax}",
        "b" => "{rbx}",
        "c" => "{rcx}",
        "d" => "{rdx}",
        "S" => "{rsi}",
        "D" => "{rdi}",
        _ => code,
    }
}
