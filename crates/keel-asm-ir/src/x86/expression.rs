//! Bounded GNU integer expressions shared by source parsing and layout.
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum Expr {
    Constant(i128),
    Symbol(String),
    Unary(u8, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
}
#[derive(Clone, Copy, Debug)]
pub struct Value {
    pub section: Option<usize>,
    pub number: i128,
}
impl Value {
    pub fn absolute(number: i128) -> Self {
        Self {
            section: None,
            number,
        }
    }
}
fn bounded(n: i128) -> Result<i128, String> {
    if n < -i128::from(u64::MAX) || n > i128::from(u64::MAX) {
        Err("expression exceeds 64 bits".into())
    } else {
        Ok(n)
    }
}
impl Expr {
    pub fn eval(
        &self,
        resolve: &mut impl FnMut(&str) -> Result<Value, String>,
    ) -> Result<Value, String> {
        match self {
            Self::Constant(n) => Ok(Value::absolute(*n)),
            Self::Symbol(s) => resolve(s),
            Self::Unary(op, e) => {
                let v = e.eval(resolve)?;
                if v.section.is_some() {
                    return Err("unary operator requires an absolute expression".into());
                }
                Ok(Value::absolute(bounded(match op {
                    b'-' => -v.number,
                    b'~' => !v.number,
                    b'!' => i128::from(v.number == 0),
                    _ => v.number,
                })?))
            }
            Self::Binary(op, a, b) => {
                let a = a.eval(resolve)?;
                let b = b.eval(resolve)?;
                let section = match op.as_str() {
                    "+" if a.section.is_none() => b.section,
                    "+" if b.section.is_none() => a.section,
                    "-" if b.section.is_none() => a.section,
                    "-" if a.section == b.section => None,
                    _ if a.section.is_none() && b.section.is_none() => None,
                    _ => {
                        return Err(
                            "expression requires absolute values or a same-section difference"
                                .into(),
                        );
                    }
                };
                let (a, b) = (a.number, b.number);
                let n = match op.as_str() {
                    "+" => a.checked_add(b),
                    "-" => a.checked_sub(b),
                    "*" => a.checked_mul(b),
                    "/" => a.checked_div(b),
                    "%" => a.checked_rem(b),
                    "<<" | ">>" => {
                        if !(0..64).contains(&b) {
                            return Err("expression shift must be in 0..64".into());
                        }
                        if op == "<<" {
                            a.checked_mul(1i128 << b)
                        } else {
                            Some(a >> b)
                        }
                    }
                    "&" => Some(a & b),
                    "|" => Some(a | b),
                    "^" => Some(a ^ b),
                    "!" => Some(a | !b),
                    "==" => Some(-i128::from(a == b)),
                    "!=" | "<>" => Some(-i128::from(a != b)),
                    "<" => Some(-i128::from(a < b)),
                    ">" => Some(-i128::from(a > b)),
                    "<=" => Some(-i128::from(a <= b)),
                    ">=" => Some(-i128::from(a >= b)),
                    "&&" => Some(i128::from(a != 0 && b != 0)),
                    "||" => Some(i128::from(a != 0 || b != 0)),
                    _ => return Err("unsupported expression operator".into()),
                }
                .ok_or("expression overflow or division by zero")?;
                Ok(Value {
                    section,
                    number: bounded(n)?,
                })
            }
        }
    }
    /// Reduce expressions representable by the public linear relocation model.
    pub fn linear(&self) -> Result<(i128, BTreeMap<String, i32>), String> {
        fn visit(
            e: &Expr,
            sign: i32,
            constant: &mut i128,
            terms: &mut BTreeMap<String, i32>,
        ) -> Result<(), String> {
            match e {
                Expr::Symbol(s) => {
                    *terms.entry(s.clone()).or_default() += sign;
                }
                Expr::Binary(op, a, b) if op == "+" || op == "-" => {
                    visit(a, sign, constant, terms)?;
                    visit(b, if op == "-" { -sign } else { sign }, constant, terms)?;
                }
                Expr::Unary(b'+', e) => visit(e, sign, constant, terms)?,
                Expr::Unary(b'-', e) => visit(e, -sign, constant, terms)?,
                _ => {
                    let n = e
                        .eval(&mut |s| {
                            Err(format!("nonlinear symbolic expression involving `{s}`"))
                        })?
                        .number;
                    *constant = bounded(
                        constant
                            .checked_add(i128::from(sign) * n)
                            .ok_or("expression overflow")?,
                    )?;
                }
            }
            Ok(())
        }
        let (mut constant, mut terms) = (0, BTreeMap::new());
        visit(self, 1, &mut constant, &mut terms)?;
        terms.retain(|_, v| *v != 0);
        Ok((constant, terms))
    }
}

pub fn parse(source: &str) -> Result<Expr, String> {
    if source.len() > 65536 {
        return Err("expression exceeds 64 KiB".into());
    }
    let mut parser = Parser {
        source,
        at: 0,
        nodes: 0,
    };
    let e = parser.expr(0, 0)?;
    parser.space();
    if parser.at != source.len() {
        return Err(format!(
            "unexpected expression input `{}`",
            &source[parser.at..]
        ));
    }
    Ok(e)
}
pub fn constant(source: &str) -> Result<i128, String> {
    parse(source)?
        .eval(&mut |s| Err(format!("undefined symbol `{s}` in constant expression")))
        .map(|v| v.number)
}
struct Parser<'a> {
    source: &'a str,
    at: usize,
    nodes: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.at += 1;
        }
    }
    fn expr(&mut self, min: u8, depth: usize) -> Result<Expr, String> {
        self.nodes += 1;
        if depth > 64 || self.nodes > 512 {
            return Err("expression nesting or node limit exceeded".into());
        }
        self.space();
        let c = *self
            .source
            .as_bytes()
            .get(self.at)
            .ok_or("expected expression")?;
        let mut left = if b"+-~!".contains(&c) {
            self.at += 1;
            Expr::Unary(c, Box::new(self.expr(7, depth + 1)?))
        } else if c == b'(' {
            self.at += 1;
            let e = self.expr(0, depth + 1)?;
            self.space();
            if self.source.as_bytes().get(self.at) != Some(&b')') {
                return Err("unclosed expression parenthesis".into());
            }
            self.at += 1;
            e
        } else {
            let start = self.at;
            if c == b'"' {
                self.at += 1;
                let mut escaped = false;
                loop {
                    let c = *self
                        .source
                        .as_bytes()
                        .get(self.at)
                        .ok_or("unterminated symbol quote")?;
                    self.at += 1;
                    if c == b'"' && !escaped {
                        break;
                    }
                    escaped = c == b'\\' && !escaped;
                }
                // A relocation suffix belongs to the symbol token.
                if self.source.as_bytes().get(self.at) == Some(&b'@') {
                    self.at += 1;
                    while self
                        .source
                        .as_bytes()
                        .get(self.at)
                        .is_some_and(|c| c.is_ascii_alphanumeric())
                    {
                        self.at += 1;
                    }
                }
            } else {
                while self
                    .source
                    .as_bytes()
                    .get(self.at)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || b"_.$@".contains(c))
                {
                    self.at += 1;
                }
            }
            if self.at == start {
                return Err(format!("invalid symbol `{}`", self.source));
            }
            let token = &self.source[start..self.at];
            if c.is_ascii_digit() {
                Expr::Constant(super::integer_literal(token)?)
            } else {
                Expr::Symbol(token.into())
            }
        };
        loop {
            self.space();
            let tail = &self.source[self.at..];
            let op = [
                "<<", ">>", "<=", ">=", "==", "!=", "<>", "&&", "||", "+", "-", "*", "/", "%", "&",
                "|", "^", "!", "<", ">",
            ]
            .into_iter()
            .find(|op| tail.starts_with(op));
            let Some(op) = op else {
                break;
            };
            // GNU precedence: shifts share multiplication's level; bitwise
            // operators bind more tightly than addition. All are left associative.
            let precedence = match op {
                "||" => 1,
                "&&" => 2,
                "==" | "!=" | "<>" | "<" | ">" | "<=" | ">=" => 3,
                "+" | "-" => 4,
                "&" | "|" | "^" | "!" => 5,
                _ => 6,
            };
            if precedence < min {
                break;
            }
            self.at += op.len();
            let right = self.expr(precedence + 1, depth + 1)?;
            left = Expr::Binary(op.into(), Box::new(left), Box::new(right));
        }
        Ok(left)
    }
}
