//! Execute independent function passes between serial module barriers.

use crate::{ClearScratch, NOT_CLEAN, OptimizationContext, Pass, Versions, analysis, canonicalize};
use keel_ir::Function;

/// Large modules amortize starting workers and publishing clean stamps.
pub(super) const MIN_FUNCTIONS: usize = 1024;
/// Each distinct pass instance occupies a bit in the clean mask.
pub(super) const MAX_PASSES: usize = u64::BITS as usize;
pub(super) const MAX_WORKERS: usize = 32;

// A batch contains only audited function passes. A worker keeps its
// analysis session and scratch across the batch for its own functions.
pub(super) fn run_batch<'a>(
    functions: &mut [Function],
    canonical: &mut [bool],
    versions: &mut Versions<'a>,
    passes: &[&'a dyn Pass],
    context: OptimizationContext,
    workers: usize,
) {
    #[derive(Default, Clone, Copy)]
    struct Outcome {
        clean: u64,
        changed: bool,
    }

    let mut slots = Vec::new();
    let mut schedule = Vec::new();
    for &pass in passes {
        debug_assert!(pass.deterministic());
        let slot = versions.slot(pass);
        let bit_index = slots.iter().position(|&s| s == slot).unwrap_or_else(|| {
            slots.push(slot);
            slots.len() - 1
        });
        schedule.push((
            pass,
            1_u64 << bit_index,
            pass.requires_canonical_form(),
            pass.produces_canonical_form(),
        ));
    }
    let mut outcomes = vec![Outcome::default(); functions.len()];
    let chunk = functions.len().div_ceil(workers);
    let input = &*versions;
    std::thread::scope(|scope| {
        let mut caller = None;
        for (shard, ((functions, canonical), outcomes)) in functions
            .chunks_mut(chunk)
            .zip(canonical.chunks_mut(chunk))
            .zip(outcomes.chunks_mut(chunk))
            .enumerate()
        {
            let slots = &slots;
            let schedule = &schedule;
            let run = move || {
                let _clear = ClearScratch;
                let _analyses = analysis::Session::new();
                for (offset, ((func, canonical), outcome)) in functions
                    .iter_mut()
                    .zip(canonical)
                    .zip(outcomes)
                    .enumerate()
                {
                    let index = shard * chunk + offset;
                    for (bit, &slot) in slots.iter().enumerate() {
                        if input.clean[slot].1.get(index) == Some(&input.version[index]) {
                            outcome.clean |= 1 << bit;
                        }
                    }
                    for &(pass, bit, needs_canonical, makes_canonical) in schedule {
                        if !pass.enabled_for(func) {
                            continue;
                        }
                        if outcome.clean & bit != 0 {
                            *canonical |= needs_canonical || makes_canonical;
                            continue;
                        }
                        if needs_canonical && !*canonical {
                            if canonicalize::run(func) {
                                outcome.changed = true;
                                outcome.clean = 0;
                            }
                            *canonical = true;
                        }
                        let context = OptimizationContext {
                            canonical: needs_canonical,
                            ..context
                        };
                        let changed = pass.run(func, &context);
                        if changed {
                            outcome.changed = true;
                            outcome.clean = 0;
                        } else {
                            outcome.clean |= bit;
                        }
                        *canonical = makes_canonical || (*canonical && !changed);
                    }
                }
            };
            if shard == 0 {
                caller = Some(run);
            } else {
                scope.spawn(run);
            }
        }
        if let Some(run) = caller {
            run();
        }
    });
    // Only equality between stamps matters. Advance once per function that
    // changed, then publish the passes known clean on its final revision.
    for (index, outcome) in outcomes.into_iter().enumerate() {
        if outcome.changed {
            versions.renew(index);
        }
        for (bit, &slot) in slots.iter().enumerate() {
            if outcome.clean & (1 << bit) != 0 {
                let clean = &mut versions.clean[slot].1;
                if clean.len() <= index {
                    clean.resize(versions.version.len(), NOT_CLEAN);
                }
                clean[index] = versions.version[index];
            }
        }
    }
}
