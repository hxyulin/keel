// thread_local! (lazy + const), std::thread::spawn/join, Arc, Atomic* and Mutex.
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

static GLOBAL_HITS: AtomicUsize = AtomicUsize::new(0);
static READY: AtomicBool = AtomicBool::new(false);

thread_local! {
    static FAST: Cell<u64> = const { Cell::new(7) };
    static LAZY: Vec<u64> = (1..=5).map(|i| i * 11).collect();
}

#[inline(never)]
fn bump(n: u64) -> u64 {
    FAST.with(|f| {
        for i in 0..n {
            f.set(f.get().wrapping_mul(31).wrapping_add(i));
        }
        f.get()
    }) ^ LAZY.with(|v| v.iter().sum::<u64>())
}

fn main() {
    let total = Arc::new(AtomicU64::new(0));
    let names = Arc::new(Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..4u64)
        .map(|t| {
            let total = Arc::clone(&total);
            let names = Arc::clone(&names);
            thread::spawn(move || {
                let local = bump(10 + t);
                total.fetch_add(local % 1000, Ordering::SeqCst);
                GLOBAL_HITS.fetch_add(1, Ordering::Relaxed);
                names.lock().unwrap().push(t);
                local
            })
        })
        .collect();
    let mut per_thread: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    per_thread.sort();
    READY.store(true, Ordering::Release);
    let mut seen = names.lock().unwrap().clone();
    seen.sort();
    let cas = GLOBAL_HITS.compare_exchange(4, 40, Ordering::AcqRel, Ordering::Acquire);
    println!(
        "tls_atomics main={} threads={per_thread:?} total={} hits={} cas={cas:?} ready={} names={seen:?}",
        bump(3),
        total.load(Ordering::SeqCst),
        GLOBAL_HITS.load(Ordering::Relaxed),
        READY.load(Ordering::Acquire)
    );
}
