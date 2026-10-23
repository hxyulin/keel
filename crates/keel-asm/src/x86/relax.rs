//! Short-branch relaxation over offsets alone. When only short branches and
//! alignment padding can change size between layout passes, the first pass
//! records where each of them (and each branch target) lay. Later passes
//! then only shift those positions, so relaxation reaches its fixpoint
//! without laying out every item again; one full pass lays out the result.
use super::{Assembler, SymId, symbolic};

/// A recorded position: `offset` in `section` on the first pass.
struct Event {
    offset: u32,
    section: u32,
    /// A label's symbol, a branch's symbolic index or an alignment's padding.
    value: u32,
    kind: Kind,
}
#[derive(Clone, Copy)]
enum Kind {
    Label,
    /// A short branch of this many bytes on the first pass.
    Branch(u8),
    /// Alignment to this power of two.
    Align(u8),
}
const _: () = assert!(std::mem::size_of::<Event>() == 16);

/// First-pass positions, or nothing once an item's size could depend on
/// layout in another way.
pub(super) struct Trace {
    events: Vec<Event>,
    /// Short branch targets, indexed by symbol: only their labels matter.
    targets: Vec<bool>,
    valid: bool,
}
impl Trace {
    pub fn new(cache: &symbolic::Cache, symbols: usize) -> Self {
        let mut targets = vec![false; symbols];
        for index in 0..cache.entries.len() as u32 {
            if cache.entries[index as usize].short
                && let Some((_, [fix, ..])) = cache.cached(index)
                && let Some(add) = fix.expr().add
            {
                targets[add as usize] = true;
            }
        }
        Self {
            events: Vec::new(),
            targets,
            valid: true,
        }
    }
    fn push(&mut self, section: usize, offset: u64, value: u32, kind: Kind) {
        match (u32::try_from(section), u32::try_from(offset)) {
            (Ok(section), Ok(offset)) => self.events.push(Event {
                offset,
                section,
                value,
                kind,
            }),
            _ => self.invalidate(),
        }
    }
    /// An item whose size may change with layout.
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.events = Vec::new();
    }
    pub fn label(&mut self, sym: SymId, section: usize, offset: u64) {
        if self.valid && self.targets.get(sym as usize) == Some(&true) {
            self.push(section, offset, sym, Kind::Label);
        }
    }
    /// A symbolic instruction laid out from its cache keeps its size unless
    /// it is a short branch or an operand folds (only a difference can,
    /// without aliases).
    pub fn symbolic(&mut self, cache: &symbolic::Cache, index: u32, section: usize, offset: u64) {
        if !self.valid {
            return;
        }
        match cache.cached(index) {
            Some((bytes, fixups)) if !fixups.iter().any(|f| f.may_fold(0)) => {
                if cache.entries[index as usize].short {
                    if fixups.is_empty() {
                        self.invalidate();
                    } else {
                        self.push(section, offset, index, Kind::Branch(bytes.len() as u8));
                    }
                }
            }
            _ => self.invalidate(),
        }
    }
    pub fn align(&mut self, power: u8, section: usize, offset: u64, padding: u64) {
        if self.valid {
            match u32::try_from(padding) {
                Ok(padding) => self.push(section, offset, padding, Kind::Align(power)),
                Err(_) => self.invalidate(),
            }
        }
    }
}

/// Widen the short branches that further layout passes would, given the
/// first pass `a` after its own widening, until none changes. Stops early
/// if the first pass had items whose size this cannot model: layout passes
/// then continue from the branches widened so far, as they would have.
pub(super) fn relax(trace: &Trace, a: &mut Assembler, cache: &mut symbolic::Cache) {
    if !trace.valid || a.aliases != 0 || !a.lebs.is_empty() || !a.prefs.is_empty() {
        return;
    }
    const UNSET: u64 = u64::MAX;
    let mut values = vec![UNSET; trace.targets.len()];
    let mut shifts = vec![0i64; a.sections.len()];
    let mut short = Vec::new();
    let mut widen = Vec::new();
    loop {
        shifts.fill(0);
        short.clear();
        for e in &trace.events {
            let shift = &mut shifts[e.section as usize];
            let at = (i64::from(e.offset) + *shift) as u64;
            match e.kind {
                Kind::Label => values[e.value as usize] = at,
                Kind::Branch(first) => match cache.cached(e.value) {
                    Some(_) if cache.entries[e.value as usize].short => {
                        short.push((e.value, e.section as usize, at))
                    }
                    Some((bytes, _)) => *shift += bytes.len() as i64 - i64::from(first),
                    None => return,
                },
                Kind::Align(power) => {
                    let padding = at.wrapping_neg() & ((1u64 << power) - 1);
                    *shift += padding as i64 - i64::from(e.value);
                }
            }
        }
        // Decide every branch before widening any, as a layout pass would.
        widen.clear();
        for &(index, section, at) in &short {
            let fix = &cache.cached(index).unwrap().1[0];
            let e = fix.expr();
            let target = e.add.unwrap();
            let fits = a.local_symbol(target, 0)
                && match a.symbols.get(target).and_then(|s| s.section) {
                    Some(s) if s == section => {
                        let value = values[target as usize];
                        if value == UNSET {
                            // Defined other than by a recorded label.
                            return;
                        }
                        let offset = at + u64::from(fix.offset);
                        (-128..=127)
                            .contains(&(i128::from(value) + e.constant - i128::from(offset)))
                    }
                    _ => false,
                };
            if !fits {
                widen.push(index);
            }
        }
        for &index in &widen {
            cache.widen(index, &mut a.symbols);
        }
        if widen.is_empty() {
            return;
        }
    }
}
