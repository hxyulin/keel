//! Compile declarative tree rules into the selector's rule table.
use std::{env, fs, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-changed=patterns.txt");
    let source = fs::read_to_string("patterns.txt").unwrap();
    let mut generated = String::from("const RULES: &[Rule] = &[\n");
    let mut hints = String::from(
        "fn generated_hint(target: Target, data: &InstData) -> Option<Value> { match (target, data) {\n",
    );
    for (line, text) in source.lines().enumerate() {
        let text = text.split('#').next().unwrap().trim();
        if text.is_empty() {
            continue;
        }
        let words: Vec<_> = text.split_whitespace().collect();
        assert_eq!(
            words.len(),
            5,
            "patterns.txt:{}: expected five fields",
            line + 1
        );
        if words[0] == "hint" {
            let (target, format, op, input) = (words[1], words[2], words[3], words[4]);
            match format {
                "binary" => {
                    let input: usize = input.parse().expect("hint operand index");
                    assert!(input < 2);
                    hints.push_str(&format!("(Target::{target}, InstData::Binary {{ op: BinaryOp::{op}, args }}) => Some(args[{input}]),\n"));
                }
                "convert" => {
                    assert_eq!(input, "0");
                    hints.push_str(&format!("(Target::{target}, InstData::Convert {{ op: ConvertOp::{op}, arg, .. }}) => Some(*arg),\n"));
                }
                _ => panic!("unknown hint format: {format}"),
            }
            continue;
        }
        let [target, root, child, machine, commute] = words.as_slice() else {
            unreachable!()
        };
        assert!(matches!(*commute, "true" | "false"));
        generated.push_str(&format!("Rule {{ target: Target::{target}, root: BinaryOp::{root}, child: BinaryOp::{child}, machine: Machine::{machine}, commute: {commute} }},\n"));
    }
    generated.push_str("];\n");
    hints.push_str("_ => None, } }\n");
    generated.push_str(&hints);
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("patterns.rs"),
        generated,
    )
    .unwrap();
}
