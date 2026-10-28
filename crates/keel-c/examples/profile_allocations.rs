//! Allocation counts only (never use this instrumented binary for timings).
//! profile_allocations INPUT.i... [--debug]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[derive(Clone, Copy, Default)]
struct Counts {
    enabled: bool,
    alloc: u64,
    realloc: u64,
    dealloc: u64,
    requested_bytes: u64,
}
thread_local! {
    static COUNTS: Cell<Counts> = const { Cell::new(Counts {
        enabled: false, alloc: 0, realloc: 0, dealloc: 0, requested_bytes: 0,
    }) };
}
struct CountingAllocator;
// SAFETY: Every operation forwards the original pointer/layout unchanged to
// System. The const-initialized TLS counter does not allocate or own memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forward the caller's allocation layout unchanged to System.
        let pointer = unsafe { System.alloc(layout) };
        COUNTS.with(|cell| {
            let mut c = cell.get();
            if c.enabled {
                c.alloc += 1;
                c.requested_bytes += layout.size() as u64;
                cell.set(c);
            }
        });
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forward the caller's allocation layout unchanged to System.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        COUNTS.with(|cell| {
            let mut c = cell.get();
            if c.enabled {
                c.alloc += 1;
                c.requested_bytes += layout.size() as u64;
                cell.set(c);
            }
        });
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        COUNTS.with(|cell| {
            let mut c = cell.get();
            if c.enabled {
                c.dealloc += 1;
                cell.set(c);
            }
        });
        // SAFETY: The caller supplies a live System allocation and its original layout.
        unsafe { System.dealloc(pointer, layout) };
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: Forward the live allocation, original layout and requested size unchanged.
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        COUNTS.with(|cell| {
            let mut c = cell.get();
            if c.enabled {
                c.realloc += 1;
                c.requested_bytes += size as u64;
                cell.set(c);
            }
        });
        pointer
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let opts = keel_c::Options {
        debug_info: args.iter().any(|s| s == "--debug"),
        ..Default::default()
    };
    let inputs: Vec<_> = args
        .iter()
        .filter(|s| s.as_str() != "--debug")
        .map(|p| Ok((p, std::fs::read_to_string(p)?)))
        .collect::<Result<_, std::io::Error>>()?;
    for (path, src) in &inputs {
        let module = keel_c::compile(src, path, opts)?;
        keel_ir::verify_module(&module).map_err(|e| format!("{path}: {e:?}"))?;
    }
    for (path, src) in &inputs {
        COUNTS.with(|c| {
            c.set(Counts {
                enabled: true,
                ..Counts::default()
            })
        });
        let module = keel_c::compile(src, path, opts)?;
        let counts = COUNTS.with(|c| {
            let n = c.get();
            c.set(Counts::default());
            n
        });
        let functions = module.functions.len();
        drop(module);
        println!(
            "{{\"path\":{path:?},\"alloc\":{},\"realloc\":{},\"dealloc\":{},\"requested_bytes\":{},\"functions\":{functions}}}",
            counts.alloc, counts.realloc, counts.dealloc, counts.requested_bytes
        );
    }
    Ok(())
}
