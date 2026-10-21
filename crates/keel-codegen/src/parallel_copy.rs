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

/// Orders the parallel copy `moves`, each `(dst, src)`, into single moves
/// that have the same effect when run one after another. Destinations must
/// be distinct, and `temp` must not appear in `moves`.
pub fn sequentialize<L: Copy + Eq>(moves: &[(L, L)], temp: L) -> Vec<(L, L)> {
    debug_assert!(
        moves
            .iter()
            .enumerate()
            .all(|(i, m)| moves[..i].iter().all(|n| n.0 != m.0)),
        "parallel copy with a repeated destination"
    );
    debug_assert!(
        moves.iter().all(|&(d, s)| d != temp && s != temp),
        "the temporary is part of the copy"
    );

    // A move onto itself does nothing (and would never become ready).
    let mut pending: Vec<(L, L)> = moves.iter().copied().filter(|(d, s)| d != s).collect();
    // Usually no move reads another's destination. Then every move is ready
    // from the start and they run in their given order.
    if pending
        .iter()
        .all(|&(dst, _)| pending.iter().all(|&(_, src)| src != dst))
    {
        return pending;
    }
    let mut out = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .position(|&(dst, _)| pending.iter().all(|&(_, src)| src != dst));
        match ready {
            Some(i) => out.push(pending.remove(i)),
            None => {
                // Only cycles are left. Save one destination so that the
                // move writing it becomes ready.
                let saved = pending[0].0;
                out.push((temp, saved));
                for (_, src) in &mut pending {
                    if *src == saved {
                        *src = temp;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::sequentialize;

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
