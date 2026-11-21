//! Track source statements and annotations on the CFG before optimization.

use super::{Block, Error, Function, Loc, branch_targets};
use keel_ir::fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};

#[derive(Default)]
pub(super) struct Diagnostics {
    switches: Vec<Switch>,
    active: Vec<usize>,
    /// Last source action in a block, ignoring compiler-generated jumps and
    /// cleanups. SSA assignments and other statements may emit no instructions.
    statements: HashMap<Block, Statement>,
}

struct Switch {
    cases: HashSet<Block>,
    exit: Block,
    incoming: Vec<(Block, Loc)>,
}

#[derive(Clone, Copy)]
enum Statement {
    Work,
    Annotated(usize),
    Goto,
}

impl Diagnostics {
    pub(super) fn enter(&mut self, cases: &[Block], exit: Block) {
        self.active.push(self.switches.len());
        self.switches.push(Switch {
            cases: cases.iter().copied().collect(),
            exit,
            incoming: Vec::new(),
        });
    }

    pub(super) fn leave(&mut self) {
        self.active.pop();
    }

    pub(super) fn case(&mut self, predecessor: Block, loc: Loc) {
        self.switches[*self.active.last().expect("case belongs to a switch")]
            .incoming
            .push((predecessor, loc));
    }

    pub(super) fn work(&mut self, block: Block) {
        if !self.active.is_empty() {
            self.statements.insert(block, Statement::Work);
        }
    }

    pub(super) fn annotation(&mut self, block: Block) {
        if let Some(&switch) = self.active.last() {
            self.statements.insert(block, Statement::Annotated(switch));
        }
    }

    pub(super) fn goto(&mut self, block: Block) {
        if !self.active.is_empty() {
            self.statements.insert(block, Statement::Goto);
        }
    }

    pub(super) fn emit(self, function: &Function, warnings: &mut Vec<Error>) {
        for (index, switch) in self.switches.into_iter().enumerate() {
            if switch.incoming.is_empty() {
                continue;
            }
            // Enter each arm by switch dispatch. Crossing a different case
            // starts a fresh arm, so stacked labels alone never warn.
            let mut pending: Vec<_> = switch.cases.iter().map(|&block| (block, false)).collect();
            let mut seen = HashSet::default();
            let mut unmarked = HashSet::default();
            while let Some((block, prior_work)) = pending.pop() {
                let work = match self.statements.get(&block) {
                    Some(Statement::Work) => true,
                    Some(Statement::Annotated(owner)) if *owner == index => false,
                    Some(Statement::Goto) => false,
                    _ => prior_work,
                };
                if !seen.insert((block, work)) {
                    continue;
                }
                if work {
                    unmarked.insert(block);
                }
                pending.extend(branch_targets(function, block).iter().filter_map(|dest| {
                    (dest.block != switch.exit && !switch.cases.contains(&dest.block))
                        .then_some((dest.block, work))
                }));
            }
            for (block, loc) in switch.incoming {
                if unmarked.contains(&block) {
                    warnings.push(Error::new(
                        loc,
                        "implicit fallthrough to this switch label [-Wimplicit-fallthrough]",
                    ));
                }
            }
        }
    }
}
