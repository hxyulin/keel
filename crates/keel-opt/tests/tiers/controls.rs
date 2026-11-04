//! Pipeline controls: pass disabling, the bisect limit and the traces.
use keel_ir::{BinaryOp, Function, InstData, Module};
use keel_opt::{OptError, OptimizeOptions, Pass, Pipeline};

const IR: &str = "func @a(i64) -> i64 {\nblock0(v0: i64):\n v1 = iconst.i64 0\n v2 = iadd v0, v1\n return v2\n}\n\
                  func @b(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = imul v0, v1\n return v2\n}\n\
                  func @c(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = isub v0, v1\n return v2\n}\n\
                  func @d(i64) -> i64 {\nblock0(v0: i64):\n v1 = iconst.i64 1\n v2 = imul v0, v1\n return v2\n}";

/// A deliberate miscompile: turns the subtraction in @c into an addition.
/// The IR stays valid, so only a semantic check finds it.
struct Broken;
impl Pass for Broken {
    fn name(&self) -> &'static str {
        "broken"
    }
    fn description(&self) -> &'static str {
        "miscompile @c"
    }
    fn run(&self, f: &mut Function, _: &keel_opt::OptimizationContext) -> bool {
        if f.name != "c" {
            return false;
        }
        let Some(inst) = f.layout_insts().map(|(_, i)| i).find(|&i| {
            matches!(
                f.inst_data(i),
                InstData::Binary {
                    op: BinaryOp::Isub,
                    ..
                }
            )
        }) else {
            return false;
        };
        if let InstData::Binary { op, .. } = f.inst_data_mut(inst) {
            *op = BinaryOp::Iadd;
        }
        true
    }
}

/// Full with `broken` after the second round's `gvn`.
fn pipeline() -> Vec<&'static dyn Pass> {
    let mut passes: Vec<&dyn Pass> = keel_opt::DEFAULT_PIPELINE
        .iter()
        .map(|name| keel_opt::find_pass(name).unwrap())
        .collect();
    let gvn = keel_opt::DEFAULT_PIPELINE
        .iter()
        .enumerate()
        .filter(|(_, name)| **name == "gvn")
        .nth(1)
        .unwrap()
        .0;
    passes.insert(gvn + 1, &Broken);
    passes
}

/// Runs the pipeline; returns the module and the trace.
fn run(passes: &[&dyn Pass], options: OptimizeOptions) -> Result<(Module, String), OptError> {
    let mut module = keel_ir::parse(IR).unwrap();
    let mut trace = Vec::new();
    Pipeline::new(passes)
        .options(options)
        .trace(&mut trace)
        .run(&mut module)?;
    Ok((module, String::from_utf8(trace).unwrap()))
}

fn good(module: &Module) -> bool {
    let c = module.functions.iter().find(|f| f.name == "c").unwrap();
    c.to_string().contains("isub")
}

fn bisect(limit: Option<usize>, verbose: bool) -> (Module, String) {
    let options = OptimizeOptions {
        bisect_limit: limit,
        bisect_verbose: verbose,
        ..Default::default()
    };
    run(&pipeline(), options).unwrap()
}

#[test]
fn bisect_limit_finds_the_broken_invocation() {
    let (full, trace) = bisect(None, true);
    assert!(!good(&full));
    let lines: Vec<&str> = trace.lines().collect();
    let total = lines.len();
    for (n, line) in lines.iter().enumerate() {
        assert!(line.starts_with(&format!("bisect: {} ", n + 1)), "{line}");
    }
    // Module passes count once.
    assert!(
        trace.contains(" inline-cheap-reserve (module)\n"),
        "{trace}"
    );
    assert!(bisect(Some(0), false).1.is_empty());
    assert!(good(&bisect(Some(0), false).0));

    // The smallest limit that miscompiles is the broken invocation.
    let (mut lo, mut hi, mut runs) = (0, total, 0);
    while lo < hi {
        let mid = (lo + hi) / 2;
        runs += 1;
        if good(&bisect(Some(mid), false).0) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    assert!(runs <= usize::BITS as usize - total.leading_zeros() as usize);
    let (_, trace) = bisect(Some(lo), true);
    let last = trace.lines().last().unwrap();
    assert_eq!(last, format!("bisect: {lo} broken @c"));
    assert_eq!(lines[lo - 1], last);
    assert_eq!(trace.lines().count(), lo);
}

#[test]
fn disabled_passes_are_skipped_and_must_exist() {
    let options = OptimizeOptions {
        disabled: vec!["broken".into(), "gvn".into()],
        bisect_verbose: true,
        ..Default::default()
    };
    let (module, trace) = run(&pipeline(), options).unwrap();
    assert!(good(&module));
    assert!(
        !trace.contains(" broken ") && !trace.contains(" gvn "),
        "{trace}"
    );
    assert!(trace.contains(" dce @c\n"), "{trace}");
    let unknown = OptimizeOptions {
        disabled: vec!["no-such-pass".into()],
        ..Default::default()
    };
    let error = run(&[&Broken], unknown).unwrap_err();
    assert!(matches!(error, OptError::UnknownPass(name) if name == "no-such-pass"));
}

#[test]
fn print_changed_shows_each_changed_function_once_per_invocation() {
    let dce = keel_opt::find_pass("dce").unwrap();
    let instcombine = keel_opt::find_pass("instcombine").unwrap();
    let passes: [&dyn Pass; 3] = [&Broken, instcombine, dce];
    let options = OptimizeOptions {
        print_changed: true,
        ..Default::default()
    };
    let (_, trace) = run(&passes, options.clone()).unwrap();
    // @b has nothing to change. @a and @d fold in instcombine and lose
    // their constants in dce; @c changes only in the broken pass.
    let headers: Vec<&str> = trace
        .lines()
        .filter(|l| l.starts_with("; after "))
        .collect();
    assert_eq!(
        headers,
        [
            "; after broken, invocation 3",
            "; after instcombine, invocation 5",
            "; after instcombine, invocation 8",
            "; after dce, invocation 9",
            "; after dce, invocation 12",
        ],
        "{trace}"
    );
    assert!(trace.contains("; after broken, invocation 3\nfunc @c("));
    assert!(!trace.contains("func @b("));
    let only_c = OptimizeOptions {
        print_function: Some("c".into()),
        ..options
    };
    let (_, trace) = run(&passes, only_c).unwrap();
    assert!(trace.starts_with("; after broken, invocation 3\nfunc @c("));
    assert_eq!(trace.matches("; after ").count(), 1, "{trace}");
}
