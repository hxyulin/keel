//! Seeded random functions over integers and control flow, checked with the
//! differential driver against every registered pass and every tier.
//!
//! A function has `i8` to `i64` parameters and returns `i64`. Its body is a
//! chain of regions: straight-line arithmetic, an if-else diamond, a switch,
//! or a counted loop (at most 7 trips, nested at most twice). Every integer
//! instruction is defined for all inputs except division, which may trap;
//! the driver compares traps too.
//!
//! The normal run checks `DEFAULT_ITERS` functions. `KEEL_FUZZ_ITERS` sets
//! the count and `KEEL_FUZZ_SEED` the first seed, so a failure reported at
//! seed `s` reruns alone with `KEEL_FUZZ_SEED=s KEEL_FUZZ_ITERS=1`.

use super::differential::{Rng, Tally, compare, configs, optimize};
use std::fmt::Write;

const DEFAULT_ITERS: u64 = 24;

const TYPES: [&str; 4] = ["i8", "i16", "i32", "i64"];
const BITS: [u32; 4] = [8, 16, 32, 64];
const I8: usize = 0;
const I32: usize = 2;
const I64: usize = 3;

const BINARY: [&str; 17] = [
    "iadd", "isub", "imul", "and", "or", "xor", "shl", "ushr", "sshr", "rotl", "rotr", "umulhi",
    "smulhi", "udiv", "sdiv", "urem", "srem",
];
const UNARY: [&str; 5] = ["ineg", "bnot", "popcnt", "clz", "ctz"];
const OVERFLOW: [&str; 6] = [
    "uadd_overflow",
    "sadd_overflow",
    "usub_overflow",
    "ssub_overflow",
    "umul_overflow",
    "smul_overflow",
];
const CONDS: [&str; 10] = [
    "eq", "ne", "ult", "ule", "ugt", "uge", "slt", "sle", "sgt", "sge",
];

struct Gen {
    rng: Rng,
    text: String,
    next_value: u32,
    next_block: u32,
    /// Values that dominate the current point: id, type index and whether
    /// it is a constant.
    pool: Vec<(u32, usize, bool)>,
}

impl Gen {
    fn value_id(&mut self) -> u32 {
        self.next_value += 1;
        self.next_value - 1
    }

    fn block_id(&mut self) -> u32 {
        self.next_block += 1;
        self.next_block - 1
    }

    fn line(&mut self, line: &str) {
        writeln!(self.text, "    {line}").unwrap();
    }

    /// Emits `vN = rhs` of type `ty` and makes it available.
    fn emit(&mut self, ty: usize, rhs: &str) -> u32 {
        let id = self.value_id();
        self.line(&format!("v{id} = {rhs}"));
        self.pool.push((id, ty, rhs.starts_with("iconst")));
        id
    }

    fn constant(&mut self, ty: usize) -> u32 {
        let bits = BITS[ty];
        let imm: i64 = match self.rng.below(6) {
            0 => 0,
            1 => 1,
            2 => -1,
            3 => i64::from(bits) - 1,
            4 => (1u64 << (bits - 1)) as i64,
            _ => self.rng.next() as i64 >> (64 - bits),
        };
        self.emit(ty, &format!("iconst.{} {imm}", TYPES[ty]))
    }

    /// A value of type `ty`: usually a computed one, often among the most
    /// recent so that chains form, sometimes a constant.
    fn operand(&mut self, ty: usize) -> u32 {
        let mut candidates: Vec<u32> = self
            .pool
            .iter()
            .filter(|&&(_, t, konst)| t == ty && !konst)
            .map(|&(id, ..)| id)
            .collect();
        if candidates.is_empty() || self.rng.below(6) == 0 {
            return self.constant(ty);
        }
        if self.rng.below(2) == 0 {
            candidates.drain(..candidates.len().saturating_sub(3));
        }
        *self.rng.pick(&candidates)
    }

    fn any_type(&mut self) -> usize {
        self.rng.below(TYPES.len())
    }

    fn instruction(&mut self) {
        let ty = self.any_type();
        match self.rng.below(10) {
            0..=3 => {
                let op = *self.rng.pick(&BINARY);
                let (a, b) = (self.operand(ty), self.operand(ty));
                self.emit(ty, &format!("{op} v{a}, v{b}"));
            }
            4 => {
                let op = *self.rng.pick(&UNARY);
                let a = self.operand(ty);
                self.emit(ty, &format!("{op} v{a}"));
            }
            5 => {
                let cond = *self.rng.pick(&CONDS);
                let (a, b) = (self.operand(ty), self.operand(ty));
                self.emit(I8, &format!("icmp.{cond} v{a}, v{b}"));
            }
            6 => {
                let c = self.operand(I8);
                let (a, b) = (self.operand(ty), self.operand(ty));
                self.emit(ty, &format!("select v{c}, v{a}, v{b}"));
            }
            7 | 8 => {
                let from = self.any_type();
                let a = self.operand(from);
                let op = match from.cmp(&ty) {
                    std::cmp::Ordering::Less if self.rng.below(2) == 0 => "sext",
                    std::cmp::Ordering::Less => "uext",
                    std::cmp::Ordering::Greater => "trunc",
                    std::cmp::Ordering::Equal => return,
                };
                self.emit(ty, &format!("{op}.{} v{a}", TYPES[ty]));
            }
            _ => {
                let op = *self.rng.pick(&OVERFLOW);
                let (a, b) = (self.operand(ty), self.operand(ty));
                let (sum, flag) = (self.value_id(), self.value_id());
                self.line(&format!("v{sum}, v{flag} = {op} v{a}, v{b}"));
                self.pool.push((sum, ty, false));
                self.pool.push((flag, I8, false));
            }
        }
    }

    fn straight(&mut self, max: usize) {
        for _ in 0..1 + self.rng.below(max) {
            self.instruction();
        }
    }

    /// Starts `block{id}` with parameters of the given types.
    fn begin(&mut self, id: u32, params: &[usize]) -> Vec<u32> {
        let ids: Vec<u32> = params.iter().map(|_| self.value_id()).collect();
        let list: Vec<String> = ids
            .iter()
            .zip(params)
            .map(|(v, &t)| format!("v{v}: {}", TYPES[t]))
            .collect();
        if list.is_empty() {
            writeln!(self.text, "block{id}:").unwrap();
        } else {
            writeln!(self.text, "block{id}({}):", list.join(", ")).unwrap();
        }
        for (&v, &t) in ids.iter().zip(params) {
            self.pool.push((v, t, false));
        }
        ids
    }

    /// An arm that computes a value of type `ty` and jumps to `join` with it.
    fn arm(&mut self, id: u32, ty: usize, join: u32, depth: u32) {
        let saved = self.pool.len();
        self.begin(id, &[]);
        self.region(depth + 1);
        let x = self.operand(ty);
        self.line(&format!("jump block{join}(v{x})"));
        self.pool.truncate(saved);
    }

    fn diamond(&mut self, depth: u32) {
        let ty = self.any_type();
        let c = self.operand(I8);
        let (then, other, join) = (self.block_id(), self.block_id(), self.block_id());
        self.line(&format!("brif v{c}, block{then}, block{other}"));
        self.arm(then, ty, join, depth);
        self.arm(other, ty, join, depth);
        self.begin(join, &[ty]);
    }

    fn switch(&mut self, depth: u32) {
        let ty = self.any_type();
        let scrutinee_ty = self.any_type();
        let s = self.operand(scrutinee_ty);
        let join = self.block_id();
        let arms: Vec<u32> = (0..2 + self.rng.below(3))
            .map(|_| self.block_id())
            .collect();
        let mut cases: Vec<i64> = Vec::new();
        while cases.len() + 1 < arms.len() {
            let case = match self.rng.below(3) {
                0 => -1,
                _ => self.rng.below(5) as i64,
            };
            if !cases.contains(&case) {
                cases.push(case);
            }
        }
        let list: Vec<String> = cases
            .iter()
            .zip(&arms[1..])
            .map(|(c, b)| format!("{c}: block{b}"))
            .collect();
        self.line(&format!(
            "switch v{s}, block{}, [{}]",
            arms[0],
            list.join(", ")
        ));
        for &arm in &arms {
            self.arm(arm, ty, join, depth);
        }
        self.begin(join, &[ty]);
    }

    fn counted_loop(&mut self, depth: u32) {
        let ty = self.any_type();
        let init = self.operand(ty);
        let limit = self.operand(I32);
        let seven = self.emit(I32, "iconst.i32 7");
        let trips = self.emit(I32, &format!("and v{limit}, v{seven}"));
        let zero = self.emit(I32, "iconst.i32 0");
        let (header, body, exit) = (self.block_id(), self.block_id(), self.block_id());
        self.line(&format!("jump block{header}(v{zero}, v{init})"));
        let params = self.begin(header, &[I32, ty]);
        let (i, acc) = (params[0], params[1]);
        let c = self.value_id();
        self.line(&format!("v{c} = icmp.ult v{i}, v{trips}"));
        self.line(&format!("brif v{c}, block{body}, block{exit}"));

        let saved = self.pool.len();
        self.begin(body, &[]);
        self.region(depth + 1);
        let op = *self.rng.pick(&BINARY[..12]);
        let x = self.operand(ty);
        let next = self.emit(ty, &format!("{op} v{acc}, v{x}"));
        let one = self.emit(I32, "iconst.i32 1");
        let step = self.emit(I32, &format!("iadd v{i}, v{one}"));
        self.line(&format!("jump block{header}(v{step}, v{next})"));
        self.pool.truncate(saved);

        // The header's parameters dominate the exit.
        self.begin(exit, &[]);
    }

    fn region(&mut self, depth: u32) {
        let choice = if depth >= 2 { 0 } else { self.rng.below(6) };
        match choice {
            0..=2 => self.straight(4),
            3 => self.diamond(depth),
            4 => self.switch(depth),
            _ => self.counted_loop(depth),
        }
    }
}

/// The text of a random function `@fuzz` for `seed`.
pub fn generate(seed: u64) -> String {
    let mut g = Gen {
        rng: Rng::new(seed),
        text: String::new(),
        next_value: 0,
        next_block: 1,
        pool: Vec::new(),
    };
    let params: Vec<usize> = (0..1 + g.rng.below(4)).map(|_| g.any_type()).collect();
    let names: Vec<&str> = params.iter().map(|&t| TYPES[t]).collect();
    writeln!(g.text, "func @fuzz({}) -> i64 {{", names.join(", ")).unwrap();
    g.begin(0, &params);
    for _ in 0..2 + g.rng.below(5) {
        g.region(0);
    }
    // Fold several computed values into the result, so most of the body
    // stays live.
    let mut result = g.operand(I64);
    for _ in 0..6 {
        let computed: Vec<(u32, usize)> = g
            .pool
            .iter()
            .filter(|&&(.., konst)| !konst)
            .map(|&(id, ty, _)| (id, ty))
            .collect();
        let (x, ty) = *g.rng.pick(&computed);
        let wide = if ty == I64 {
            x
        } else {
            g.emit(I64, &format!("uext.i64 v{x}"))
        };
        result = g.emit(I64, &format!("xor v{result}, v{wide}"));
    }
    g.line(&format!("return v{result}"));
    g.text.push_str("}\n");
    g.text
}

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name).map_or(default, |v| {
        v.parse()
            .unwrap_or_else(|_| panic!("{name} must be an integer, not {v:?}"))
    })
}

#[test]
fn random_functions_keep_their_results() {
    let first = env("KEEL_FUZZ_SEED", 0);
    let iters = env("KEEL_FUZZ_ITERS", DEFAULT_ITERS);
    let configs = configs();
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    for seed in first..first + iters {
        let src = generate(seed);
        let module = keel_ir::parse(&src).unwrap_or_else(|e| panic!("seed {seed}: {e}\n{src}"));
        if let Err(e) = keel_ir::verify_module(&module) {
            panic!("seed {seed}: {e:?}\n{src}");
        }
        for &config in &configs {
            let result = optimize(&module, config)
                .and_then(|after| compare(&module, &after, seed, &mut tally));
            if let Err(e) = result {
                failures.push(format!("seed {seed}, {config:?}: {e}\n{src}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(tally.compared > 0, "{tally:?}");
}
