//! Monotone branch relaxation using final instruction and data offsets.
use super::*;
use keel_ir::fxhash::FxHashMap;

fn target(line: &str) -> Option<&str> {
    let line = line.trim();
    let (op, args) = line.split_once(char::is_whitespace)?;
    if op.starts_with("b.") {
        Some(args.trim())
    } else if matches!(op, "cbz" | "cbnz") {
        args.split_once(',').map(|(_, label)| label.trim())
    } else {
        None
    }
}

// Unknown directives or compound inline-asm statements break the known layout.
// Branches crossing one must use the conservative long form. Alignment is
// bounded by its maximum padding, since the function's section offset is unknown.
fn size(line: &str) -> Option<u64> {
    let line = line.trim();
    if line == "; keel-inline-boundary" {
        return None;
    }
    if line.is_empty() || line.ends_with(':') || line.starts_with([';', '#']) {
        return Some(0);
    }
    if line.contains(';') || line.contains("//") {
        return None;
    }
    let (op, args) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let args = args.trim();
    match op {
        ".byte" | ".short" | ".long" | ".quad" => {
            let width = match op {
                ".byte" => 1,
                ".short" => 2,
                ".long" => 4,
                _ => 8,
            };
            Some(width * args.split(',').count() as u64)
        }
        ".space" | ".zero" => args.split(',').next()?.trim().parse().ok(),
        ".p2align" => {
            let power: u32 = args.parse().ok()?;
            1u64.checked_shl(power).map(|n| n - 1)
        }
        op if op.starts_with(".cfi_") || matches!(op, ".loc" | ".globl" | ".type" | ".size") => {
            Some(0)
        }
        op if op.starts_with('.') => None,
        _ => Some(4),
    }
}

fn plan(lines: &[String], range: u64) -> Vec<bool> {
    let mut long = vec![false; lines.len()];
    loop {
        let mut labels = FxHashMap::default();
        let mut positions = Vec::with_capacity(lines.len());
        let (mut offset, mut epoch) = (0u64, 0usize);
        for (i, line) in lines.iter().enumerate() {
            positions.push((epoch, offset));
            if let Some(label) = line.trim().strip_suffix(':') {
                labels.insert(label, (epoch, offset));
            }
            match size(line) {
                Some(bytes) => offset = offset.saturating_add(bytes + if long[i] { 4 } else { 0 }),
                None => {
                    epoch += 1;
                    offset = 0;
                }
            }
        }
        let mut changed = false;
        for (i, line) in lines.iter().enumerate() {
            if long[i] {
                continue;
            }
            if let Some(target) = target(line) {
                let (epoch, from) = positions[i];
                let fits = labels.get(target).is_some_and(|&(to_epoch, to)| {
                    if epoch != to_epoch {
                        return false;
                    }
                    // Positive imm19 ends at range - 4; negative includes -range.
                    if to >= from {
                        to - from <= range - 4
                    } else {
                        from - to <= range
                    }
                });
                if !fits {
                    long[i] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            return long;
        }
    }
}

fn rewrite(lines: &[String], long: &[bool], name: &str, n: &mut usize) -> String {
    let mut out = String::new();
    for (line, &expand) in lines.iter().zip(long) {
        if line.trim() == "; keel-inline-boundary" {
            continue;
        }
        if expand {
            let target = target(line).expect("only conditional branches expand");
            let (op, args) = line
                .trim()
                .split_once(char::is_whitespace)
                .expect("branch operands");
            let near = format!("L{name}_near{n}");
            *n += 1;
            if let Some(cond) = op.strip_prefix("b.") {
                writeln!(out, "\tb.{}\t{near}", super::super::invert_cond(cond)).unwrap();
            } else {
                let (reg, _) = args.split_once(',').expect("compare branch register");
                writeln!(
                    out,
                    "\t{}\t{}, {near}",
                    if op == "cbz" { "cbnz" } else { "cbz" },
                    reg.trim()
                )
                .unwrap();
            }
            writeln!(out, "\tb\t{target}\n{near}:").unwrap();
        } else {
            writeln!(out, "{line}").unwrap();
        }
    }
    out
}

pub(crate) fn relax_source(text: &mut String, name: &str) {
    let lines: Vec<_> = text.lines().map(str::to_owned).collect();
    let long = plan(&lines, 1 << 20);
    if long.iter().any(|&v| v)
        || lines
            .iter()
            .any(|line| line.trim() == "; keel-inline-boundary")
    {
        *text = rewrite(&lines, &long, name, &mut 0);
    }
}

pub(super) fn relax_program(program: &mut Program, name: &str) {
    // Only metadata and symbolic operands need formatting for layout analysis.
    let mut lines = Vec::new();
    let mut spans = Vec::new();
    for item in &program.items {
        let start = lines.len();
        match item {
            Item::Label(label) => lines.push(format!("{label}:")),
            Item::Symbolic(s) => lines.push(s.to_string()),
            Item::Directive(d) => lines.push(d.to_string()),
            Item::Text(text) => lines.extend(text.lines().map(str::to_owned)),
            _ => lines.push("nop".into()),
        }
        spans.push(start..lines.len());
    }
    let long = plan(&lines, 1 << 20);
    let mut n = 0;
    let mut items = Vec::new();
    for (item, span) in std::mem::take(&mut program.items).into_iter().zip(spans) {
        match item {
            Item::Symbolic(s) if long[span.start] => {
                let near = format!("L{name}_near{n}");
                n += 1;
                items.push(Item::Symbolic(
                    s.inverted_to(near.clone()).expect("conditional branch"),
                ));
                items.push(Item::Branch {
                    link: false,
                    target: s.target().into(),
                });
                items.push(Item::Label(near));
            }
            Item::Text(_) => items.push(Item::Text(rewrite(
                &lines[span.clone()],
                &long[span],
                name,
                &mut n,
            ))),
            other => items.push(other),
        }
    }
    program.items = items;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_boundaries_and_local_branches_in_large_functions() {
        for (space, expected) in [(24, false), (28, true)] {
            let lines = [
                "b.eq far".into(),
                format!(".space {space}"),
                "far:".into(),
                "cbz x0, far".into(),
            ];
            assert_eq!(plan(&lines, 32), [expected, false, false, false]);
        }
        let lines = ["back:".into(), ".space 32".into(), "b.ne back".into()];
        assert!(!plan(&lines, 32)[2]);
    }
    #[test]
    fn expansion_can_push_another_branch_out_of_range() {
        let lines = ["b.eq end", "b.ne missing", ".space 20", "end:"].map(str::to_owned);
        assert_eq!(plan(&lines, 28), [true, true, false, false]);
    }
    #[test]
    fn unknown_asm_and_alignment_are_conservative() {
        let lines = ["b.eq end", ".unknown 200", "end:", "b.eq end"].map(str::to_owned);
        assert_eq!(plan(&lines, 32), [true, false, false, false]);
        let lines = ["b.eq end", ".p2align 5", "end:"].map(str::to_owned);
        assert!(plan(&lines, 32)[0]);
    }
}

#[cfg(test)]
mod routes {
    use super::*;
    #[test]
    fn structured_and_text_make_the_same_relaxation_choice() {
        use keel_asm_ir::{Directive, SymbolicInstruction as S};
        let mut p = Program::new();
        p.symbolic(S::Conditional {
            condition: 0,
            target: "far".into(),
        });
        p.directive(Directive::Space(1 << 20));
        p.label("far");
        p.symbolic(S::Compare {
            register: 1,
            is64: true,
            nonzero: false,
            target: "far".into(),
        });
        relax_program(&mut p, "test");
        assert_eq!(p.items.len(), 6);
        assert!(matches!(
            &p.items[5],
            Item::Symbolic(S::Compare { nonzero: false, .. })
        ));
        let mut text = "\tb.eq\tfar\n\t.space 1048576\nfar:\n\tcbz\tx1, far\n".to_owned();
        relax_source(&mut text, "test");
        assert_eq!(
            text,
            "\tb.ne\tLtest_near0\n\tb\tfar\nLtest_near0:\n\t.space 1048576\nfar:\n\tcbz\tx1, far\n"
        );
    }
}
