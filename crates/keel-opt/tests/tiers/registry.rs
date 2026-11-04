//! The registered passes and the pass table in docs/compiler/optimizer.md.

const OPTIMIZER_MD: &str = include_str!("../../../../docs/compiler/optimizer.md");

/// Backticked names in the first cell of each row of the "Passes" table.
fn documented_passes() -> Vec<&'static str> {
    let table = OPTIMIZER_MD
        .split_once("\n## Passes\n")
        .expect("optimizer.md has a Passes section")
        .1;
    let table = table.split("\n## ").next().unwrap();
    table
        .lines()
        .filter(|line| line.starts_with("| `"))
        .flat_map(|row| {
            row[2..]
                .split(" |")
                .next()
                .unwrap()
                .split('`')
                .skip(1)
                .step_by(2)
        })
        .collect()
}

#[test]
fn every_registered_pass_has_a_row_in_the_optimizer_reference() {
    let documented = documented_passes();
    for pass in keel_opt::PASSES {
        assert!(
            documented.contains(&pass.name()),
            "`{}` is missing from the pass table in docs/compiler/optimizer.md",
            pass.name()
        );
    }
    for name in documented {
        assert!(
            keel_opt::find_pass(name).is_some(),
            "docs/compiler/optimizer.md lists `{name}`, which is not a registered pass"
        );
    }
}
