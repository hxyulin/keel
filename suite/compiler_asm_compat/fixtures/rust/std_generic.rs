// std Vec/String/BTreeMap, generics with trait bounds, trait objects, iterators, closures.
use std::collections::BTreeMap;
use std::fmt::Write;
use std::hint::black_box;

trait Shape {
    fn area(&self) -> f64;
    fn label(&self) -> String {
        format!("shape:{:.1}", self.area())
    }
}
struct Circle(f64);
struct Rect {
    w: f64,
    h: f64,
}
impl Shape for Circle {
    fn area(&self) -> f64 {
        3.0 * self.0 * self.0
    }
}
impl Shape for Rect {
    fn area(&self) -> f64 {
        self.w * self.h
    }
    fn label(&self) -> String {
        format!("rect{}x{}", self.w, self.h)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(i64),
    Word(String),
    Sym(char),
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for part in input.split_whitespace() {
        if let Ok(n) = part.parse::<i64>() {
            out.push(Token::Num(n));
        } else if part.len() == 1 && !part.chars().all(char::is_alphanumeric) {
            out.push(Token::Sym(part.chars().next().unwrap()));
        } else {
            out.push(Token::Word(part.to_uppercase()));
        }
    }
    out
}

fn largest<T: PartialOrd + Copy>(items: &[T]) -> Option<T> {
    let mut iter = items.iter().copied();
    let first = iter.next()?;
    Some(iter.fold(first, |m, x| if x > m { x } else { m }))
}

struct Stack<T> {
    items: Vec<T>,
}
impl<T: Clone + std::fmt::Debug> Stack<T> {
    fn new() -> Self {
        Stack { items: Vec::with_capacity(2) }
    }
    fn push(&mut self, v: T) -> &mut Self {
        self.items.push(v);
        self
    }
    fn describe(&self) -> String {
        format!("{:?}", self.items)
    }
}

fn main() {
    let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Circle(1.5)), Box::new(Rect { w: 2.0, h: 4.5 }), Box::new(Circle(0.5))];
    let labels: Vec<String> = shapes.iter().map(|s| s.label()).collect();
    let total: f64 = shapes.iter().map(|s| s.area()).sum();
    let tokens = tokenize(black_box("let x = 42 + y7 * -3 ;"));
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for t in &tokens {
        let key = match t {
            Token::Num(_) => "num".to_string(),
            Token::Word(w) => format!("word{}", w.len()),
            Token::Sym(c) => format!("sym{c}"),
        };
        *counts.entry(key).or_default() += 1;
    }
    let mut text = String::new();
    for (k, v) in &counts {
        write!(text, "{k}={v};").unwrap();
    }
    let nums: Vec<i64> = tokens.iter().filter_map(|t| if let Token::Num(n) = t { Some(*n) } else { None }).collect();
    let mut stack = Stack::new();
    stack.push("a").push("bb").push("ccc");
    let sorted = {
        let mut v: Vec<u32> = (0..50u32).map(|i| black_box(i).wrapping_mul(2654435761) % 1000).collect();
        v.sort_unstable();
        v.dedup();
        v.len()
    };
    println!(
        "std_generic {:?} total={total:.3} {text} nums={nums:?} max={:?} {} {} sorted={sorted}",
        labels,
        largest(&nums),
        stack.describe(),
        tokens.len()
    );
}
