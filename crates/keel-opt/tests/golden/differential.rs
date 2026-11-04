//! Differential checks on the reference interpreter: every registered pass on
//! its own, and every tier, must keep what the IR fixtures compute.
//!
//! Each exported function with integer and pointer parameters runs on a fixed
//! set of inputs before and after optimization. Each pointer parameter gets
//! its own patterned buffer. A run that returned before must return the same
//! values and leave the same buffer contents after, and a run that trapped
//! before must still trap (the trap kind and address may change). Runs that
//! hit the step or call-depth limit, or use something the interpreter does
//! not model, on either side are skipped, as are functions whose signature a
//! pass changed.

use keel_ir::{Function, Module, Type};
use keel_ir_interp::{Interpreter, Outcome, Trap, Val};
use keel_opt::OptimizationTier;
use std::path::Path;

/// Bytes in each pointer argument's buffer.
const BUFFER: usize = 256;
/// Instructions per call. Fixture loops bounded by an input stop early with
/// the step limit, which the comparison skips.
const STEPS: u64 = 10_000;
/// Nested calls per call. Deep recursion is a resource limit, like the step
/// limit, so the comparison skips it.
const DEPTH: usize = 64;
/// Input tuples per function.
const INPUTS: usize = 8;

pub const TIERS: [OptimizationTier; 5] = [
    OptimizationTier::Fast,
    OptimizationTier::Basic,
    OptimizationTier::Balanced,
    OptimizationTier::Size,
    OptimizationTier::Full,
];

/// One optimization under test.
#[derive(Clone, Copy, Debug)]
pub enum Config {
    Pass(&'static str),
    Tier(OptimizationTier),
}

/// Every registered pass by name, then every tier.
pub fn configs() -> Vec<Config> {
    keel_opt::PASSES
        .iter()
        .map(|p| Config::Pass(p.name()))
        .chain(TIERS.map(Config::Tier))
        .collect()
}

pub fn optimize(module: &Module, config: Config) -> Result<Module, String> {
    let mut module = module.clone();
    let result = match config {
        Config::Pass(name) => keel_opt::run_passes(&mut module, &[name]),
        Config::Tier(tier) => keel_opt::optimize_with_tier(&mut module, tier),
    };
    result.map(|()| module).map_err(|e| e.to_string())
}

/// A small deterministic generator (SplitMix64).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A value below `n`, which must not be zero.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// An integer input: usually a boundary or small value, sometimes any.
    pub fn input(&mut self) -> u64 {
        const EDGES: [u64; 12] = [
            0,
            1,
            2,
            3,
            7,
            8,
            31,
            u64::MAX,
            u64::MAX - 1,
            0x80,
            0x8000_0000,
            1 << 63,
        ];
        if self.below(4) == 0 {
            self.next()
        } else {
            *self.pick(&EDGES)
        }
    }
}

/// Deterministic input tuples for `func`, the same for every configuration.
pub fn inputs(func: &Function, seed: u64) -> Vec<Vec<u64>> {
    let mut rng = Rng::new(seed);
    let arity = func.sig.params.len();
    (0..INPUTS)
        .map(|_| (0..arity).map(|_| rng.input()).collect())
        .collect()
}

/// How a call ended and what it left in the pointer arguments' buffers.
#[derive(Debug, PartialEq)]
pub struct Run {
    pub outcome: Outcome,
    pub buffers: Vec<Vec<u8>>,
}

/// Whether the differential driver can call `func` directly.
pub fn callable(func: &Function) -> bool {
    !func.local
        && !func.sig.variadic
        && func.entry_block().is_some()
        && func
            .sig
            .params
            .iter()
            .all(|p| p.ty.is_int() || p.ty == Type::Ptr)
}

/// Calls `func` in `module` on `inputs`. Each pointer parameter receives a
/// new buffer whose bytes derive from its input.
pub fn run(module: &Module, func: &Function, inputs: &[u64]) -> Run {
    let mut interp = Interpreter::new(module)
        .with_step_limit(STEPS)
        .with_depth_limit(DEPTH);
    let mut buffers = Vec::new();
    let args: Vec<Val> = func
        .sig
        .params
        .iter()
        .zip(inputs)
        .map(|(param, &input)| {
            if param.ty == Type::Ptr {
                let bytes: Vec<u8> = (0..BUFFER)
                    .map(|i| (input.rotate_left(i as u32 % 64) as u8) ^ i as u8)
                    .collect();
                let addr = interp.memory_mut().alloc_bytes(&bytes, 16);
                buffers.push(addr);
                Val::Ptr(addr)
            } else {
                Val::int(param.ty, i128::from(input))
            }
        })
        .collect();
    let outcome = interp.call(&func.name, &args);
    let buffers = buffers
        .into_iter()
        .map(|addr| interp.memory().read(addr, BUFFER as u64).unwrap().to_vec())
        .collect();
    Run { outcome, buffers }
}

/// Runs compared and skipped by [`compare`].
#[derive(Default, Debug)]
pub struct Tally {
    pub compared: usize,
    pub skipped: usize,
}

/// Compares every callable function of `before` with its copy in `after`.
/// Returns a description of the first mismatch.
pub fn compare(
    before: &Module,
    after: &Module,
    seed: u64,
    tally: &mut Tally,
) -> Result<(), String> {
    for func in before.functions.iter().filter(|f| callable(f)) {
        let Some(optimized) = after.functions.iter().find(|f| f.name == func.name) else {
            tally.skipped += 1;
            continue;
        };
        if optimized.sig != func.sig {
            tally.skipped += 1;
            continue;
        }
        for input in inputs(func, seed) {
            let expected = run(before, func, &input);
            let actual = run(after, optimized, &input);
            let comparable = |o: &Outcome| {
                matches!(o, Outcome::Return(_))
                    || matches!(o, Outcome::Trap(trap) if *trap != Trap::StackOverflow)
            };
            if !comparable(&expected.outcome) || !comparable(&actual.outcome) {
                tally.skipped += 1;
                continue;
            }
            tally.compared += 1;
            let same = match (&expected.outcome, &actual.outcome) {
                (Outcome::Trap(_), Outcome::Trap(_)) => true,
                (Outcome::Return(_), Outcome::Return(_)) => expected == actual,
                _ => false,
            };
            if !same {
                return Err(format!(
                    "@{} on {input:x?}: expected {:?}, got {:?}{}",
                    func.name,
                    expected.outcome,
                    actual.outcome,
                    if expected.outcome == actual.outcome {
                        " (pointer buffers differ)"
                    } else {
                        ""
                    }
                ));
            }
        }
    }
    Ok(())
}

/// The fixtures under `tests/golden` and `tests/fixtures` that parse and
/// verify, by file name.
fn fixtures() -> Vec<(String, Module)> {
    fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|e| e == "keel" || e == "ir") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut paths = Vec::new();
    collect(&root.join("golden"), &mut paths);
    collect(&root.join("fixtures"), &mut paths);
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let module = keel_ir::parse(&std::fs::read_to_string(&path).unwrap()).ok()?;
            keel_ir::verify_module(&module).ok()?;
            let name = path.strip_prefix(&root).unwrap().display().to_string();
            Some((name, module))
        })
        .collect()
}

#[test]
fn passes_and_tiers_preserve_fixture_results() {
    let fixtures = fixtures();
    assert!(fixtures.len() >= 10, "found {} fixtures", fixtures.len());
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    for config in configs() {
        for (seed, (name, module)) in fixtures.iter().enumerate() {
            let result = optimize(module, config)
                .and_then(|after| compare(module, &after, seed as u64, &mut tally));
            if let Err(e) = result {
                failures.push(format!("{config:?} on {name}: {e}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Guards against a driver that silently skips everything.
    assert!(tally.compared >= 1_000, "{tally:?}");
}

#[test]
fn a_changed_result_trap_or_store_is_a_mismatch() {
    let base = "func @f(i64, ptr) -> i64 {
block0(v0: i64, v1: ptr):
    v2 = iconst.i64 3
    v3 = udiv v2, v0
    store v3, v1
    return v3
}
";
    let before = keel_ir::parse(base).unwrap();
    for (from, to) in [
        ("return v3", "return v2"),
        ("udiv v2, v0", "iadd v2, v0"),
        ("store v3, v1", "store v2, v1"),
    ] {
        let after = keel_ir::parse(&base.replace(from, to)).unwrap();
        let mut tally = Tally::default();
        assert!(compare(&before, &after, 0, &mut tally).is_err(), "{to}");
    }
    let mut tally = Tally::default();
    compare(&before, &before, 0, &mut tally).unwrap();
    assert_eq!(tally.compared, INPUTS);
}
