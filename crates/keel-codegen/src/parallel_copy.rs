//! Turning a parallel copy into a sequence of ordinary moves.
//!
//! On a CFG edge every block argument moves into its parameter's location at
//! the same instant: a loop that ends in `jump block1(v4, v3)` while
//! `block1(v3, v4)` swaps two values. The machine only has one-at-a-time
//! moves, so if we copied `v3 <- v4` first, the old `v3` would be gone before
//! `v4 <- v3` could read it.
//!
//! Each move is an edge `src -> dst` in a small graph. Destinations are
//! distinct, so every location has at most one incoming edge, and the graph
//! is a set of cycles with trees hanging off them:
//!
//! 1. A move is *ready* when no other pending move still reads its
//!    destination. Emitting it destroys nothing anyone needs. Repeating this
//!    peels the trees off from the leaves inwards.
//! 2. When nothing is ready, every remaining move is on a cycle
//!    (`a <- b <- c <- a`). Copy one destination into a temporary and let its
//!    readers read the temporary instead. The cycle becomes a chain, and
//!    step 1 finishes it.
//!
//! One temporary is enough: a chain made by step 2 always has a ready move,
//! so step 1 drains it (freeing the temporary) before step 2 runs again.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::hash::Hash;

/// Copies up to this size check for independence pairwise, which needs no
/// allocation; larger ones go through [`sequentialize`]'s linear check.
const PAIRWISE_LIMIT: usize = 16;

/// Runs `emit` on each of [`sequentialize`]'s moves, in order. Most copies
/// have no move reading another's destination; those need no list.
pub fn for_each_move<L: Copy + Eq + Hash>(moves: &[(L, L)], temp: L, mut emit: impl FnMut(L, L)) {
    let independent = moves.len() <= PAIRWISE_LIMIT
        && moves
            .iter()
            .all(|&(dst, src)| dst == src || moves.iter().all(|&(d, s)| d == s || s != dst));
    if independent {
        for &(dst, src) in moves {
            if dst != src {
                emit(dst, src);
            }
        }
        return;
    }
    for (dst, src) in sequentialize(moves, temp) {
        emit(dst, src);
    }
}

/// Orders the parallel copy `moves`, each `(dst, src)`, into single moves
/// that have the same effect when run one after another. Destinations must
/// be distinct, and `temp` must not appear in `moves`.
///
/// Among the ready moves, the one given first runs first, and a cycle is
/// broken at its earliest pending move. This takes O(n log n) time.
pub fn sequentialize<L: Copy + Eq + Hash>(moves: &[(L, L)], temp: L) -> Vec<(L, L)> {
    debug_assert!(
        moves.iter().all(|&(d, s)| d != temp && s != temp),
        "the temporary is part of the copy"
    );

    // A move onto itself does nothing (and would never become ready).
    let pending: Vec<(L, L)> = moves.iter().copied().filter(|(d, s)| d != s).collect();

    // Number the locations, and record each one's writer and readers.
    let mut ids: HashMap<L, usize> = HashMap::with_capacity(2 * pending.len());
    let mut writer: Vec<Option<usize>> = Vec::new();
    let mut readers: Vec<Vec<usize>> = Vec::new();
    let mut id = |loc: L, writer: &mut Vec<Option<usize>>, readers: &mut Vec<Vec<usize>>| {
        *ids.entry(loc).or_insert_with(|| {
            writer.push(None);
            readers.push(Vec::new());
            writer.len() - 1
        })
    };
    let mut ends = Vec::with_capacity(pending.len());
    for (i, &(dst, src)) in pending.iter().enumerate() {
        let d = id(dst, &mut writer, &mut readers);
        let s = id(src, &mut writer, &mut readers);
        debug_assert!(
            writer[d].is_none(),
            "parallel copy with a repeated destination"
        );
        writer[d] = Some(i);
        readers[s].push(i);
        ends.push((d, s));
    }
    // Usually no move reads another's destination. Then every move is ready
    // from the start and they run in their given order.
    if ends.iter().all(|&(d, _)| readers[d].is_empty()) {
        return pending;
    }

    // How many pending moves still read each location.
    let mut unread: Vec<usize> = readers.iter().map(Vec::len).collect();
    let mut reads_temp = vec![false; pending.len()];
    let mut done = vec![false; pending.len()];
    let mut ready: BinaryHeap<Reverse<usize>> = (0..pending.len())
        .filter(|&i| unread[ends[i].0] == 0)
        .map(Reverse)
        .collect();
    let mut first = 0;
    let mut out = Vec::with_capacity(pending.len() + 1);
    loop {
        while first < pending.len() && done[first] {
            first += 1;
        }
        if first == pending.len() {
            break;
        }
        if let Some(Reverse(i)) = ready.pop() {
            let (dst, src) = pending[i];
            done[i] = true;
            if reads_temp[i] {
                out.push((dst, temp));
                continue;
            }
            out.push((dst, src));
            let s = ends[i].1;
            unread[s] -= 1;
            if unread[s] == 0
                && let Some(w) = writer[s]
                && !done[w]
            {
                ready.push(Reverse(w));
            }
        } else {
            // Only cycles are left. Save one destination so that the move
            // writing it becomes ready.
            let (saved, d) = (pending[first].0, ends[first].0);
            out.push((temp, saved));
            for &r in &readers[d] {
                if !done[r] {
                    reads_temp[r] = true;
                }
            }
            unread[d] = 0;
            ready.push(Reverse(first));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{for_each_move, sequentialize};

    const TEMP: u8 = 99;

    /// Runs `moves` one at a time on locations that start out holding their
    /// own number.
    fn run_sequential(moves: &[(u8, u8)], n: u8) -> Vec<u8> {
        let mut state: Vec<u8> = (0..=TEMP).collect();
        for &(dst, src) in moves {
            state[dst as usize] = state[src as usize];
        }
        state.truncate(n as usize);
        state
    }

    /// What the parallel copy means: every destination gets its source's
    /// value from before the copy.
    fn run_parallel(moves: &[(u8, u8)], n: u8) -> Vec<u8> {
        let mut state: Vec<u8> = (0..n).collect();
        for &(dst, src) in moves {
            state[dst as usize] = src;
        }
        state
    }

    /// Every parallel copy over four locations: each location is either
    /// left alone or copied from one of the four (5^4 cases). This covers
    /// swaps, 3- and 4-cycles, self-moves, one source fanning out to several
    /// destinations, and trees hanging off cycles.
    #[test]
    fn all_copies_over_four_locations() {
        const N: u8 = 4;
        for code in 0..5u32.pow(N as u32) {
            let mut moves = Vec::new();
            let mut c = code;
            for dst in 0..N {
                let choice = (c % 5) as u8;
                c /= 5;
                if choice > 0 {
                    moves.push((dst, choice - 1));
                }
            }
            let seq = sequentialize(&moves, TEMP);
            assert_eq!(
                run_sequential(&seq, N),
                run_parallel(&moves, N),
                "moves {moves:?} sequentialized as {seq:?}"
            );
            // Each cycle costs one extra move, and there are at most N/2.
            assert!(seq.len() <= moves.len() + N as usize / 2);
            let mut emitted = Vec::new();
            for_each_move(&moves, TEMP, |dst, src| emitted.push((dst, src)));
            assert_eq!(emitted, seq);
        }
    }

    /// The original quadratic-per-step order: repeatedly emit the first
    /// ready move, or save the first pending destination.
    fn reference(moves: &[(u8, u8)]) -> Vec<(u8, u8)> {
        let mut pending: Vec<(u8, u8)> = moves.iter().copied().filter(|(d, s)| d != s).collect();
        let mut out = Vec::new();
        while !pending.is_empty() {
            let ready = pending
                .iter()
                .position(|&(dst, _)| pending.iter().all(|&(_, src)| src != dst));
            match ready {
                Some(i) => out.push(pending.remove(i)),
                None => {
                    let saved = pending[0].0;
                    out.push((TEMP, saved));
                    for (_, src) in &mut pending {
                        if *src == saved {
                            *src = TEMP;
                        }
                    }
                }
            }
        }
        out
    }

    /// Every copy over five locations, in every destination order, keeps
    /// the reference order.
    #[test]
    fn matches_the_reference_order() {
        const N: u8 = 5;
        for code in 0..6u32.pow(N as u32) {
            let mut moves = Vec::new();
            let mut c = code;
            for dst in 0..N {
                let choice = (c % 6) as u8;
                c /= 6;
                if choice > 0 {
                    moves.push((dst, choice - 1));
                }
            }
            // Rotate so destinations do not always come in increasing order.
            let len = moves.len().max(1);
            moves.rotate_left(code as usize % len);
            assert_eq!(
                sequentialize(&moves, TEMP),
                reference(&moves),
                "moves {moves:?}"
            );
        }
    }

    /// Copies larger than the pairwise limit: rotations of long cycles with
    /// chains hanging off them.
    #[test]
    fn large_copies_match_the_reference_order() {
        for n in [17u8, 40, 90] {
            for step in 1..5u8 {
                let mut moves: Vec<(u8, u8)> = (0..n).map(|d| (d, (d + step) % n)).collect();
                moves.reverse();
                moves.swap(0, n as usize / 2);
                let seq = sequentialize(&moves, TEMP);
                assert_eq!(seq, reference(&moves));
                assert_eq!(run_sequential(&seq, n), run_parallel(&moves, n));
                let mut emitted = Vec::new();
                for_each_move(&moves, TEMP, |dst, src| emitted.push((dst, src)));
                assert_eq!(emitted, seq);
            }
        }
    }

    #[test]
    fn swap_uses_the_temporary() {
        assert_eq!(
            sequentialize(&[(0, 1), (1, 0)], TEMP),
            [(TEMP, 0), (0, 1), (1, TEMP)]
        );
    }

    #[test]
    fn chains_need_no_temporary() {
        // 2 <- 1 <- 0 must copy 2 first.
        assert_eq!(sequentialize(&[(1, 0), (2, 1)], TEMP), [(2, 1), (1, 0)]);
        assert_eq!(sequentialize(&[(3, 3)], TEMP), []);
    }
}
