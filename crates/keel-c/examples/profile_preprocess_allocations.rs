//! Allocation counts only (never use this instrumented binary for timings).
//! profile_preprocess_allocations APP SDK INPUT.c...
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    path::{Path, PathBuf},
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
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 4 {
        return Err("expected APP SDK INPUT.c...".into());
    }
    let mut opts = keel_c::PreprocessOptions {
        sysroot: Some(PathBuf::from(&args[2])),
        ..Default::default()
    };
    let defines: &[(&str, &str)] = match args[1].as_str() {
        "lua" => &[("LUA_USE_MACOSX", "1")],
        "quickjs" => &[
            ("_GNU_SOURCE", "1"),
            ("CONFIG_VERSION", "\"2024-01-13\""),
            ("CONFIG_BIGNUM", "1"),
        ],
        "sqlite" => &[("SQLITE_THREADSAFE", "1")],
        _ => return Err("expected lua|quickjs|sqlite".into()),
    };
    opts.defines = defines
        .iter()
        .map(|(k, v)| (k.to_string(), Some(v.to_string())))
        .collect();
    if let Some(p) = args[3..].iter().find(|p| p.ends_with("/quickjs.c")) {
        opts.include_dirs
            .push(Path::new(p).parent().unwrap().into());
    }
    for path in &args[3..] {
        let source = std::fs::read_to_string(path)?;
        drop(keel_c::preprocess(&source, path, &opts)?);
        COUNTS.with(|c| {
            c.set(Counts {
                enabled: true,
                ..Default::default()
            })
        });
        let output = keel_c::preprocess(&source, path, &opts);
        let counts = COUNTS.with(|cell| {
            let counts = cell.get();
            cell.set(Counts::default());
            counts
        });
        let output = output?;
        println!(
            "{{\"path\":{path:?},\"alloc\":{},\"realloc\":{},\"dealloc\":{},\"requested_bytes\":{},\"output_bytes\":{}}}",
            counts.alloc,
            counts.realloc,
            counts.dealloc,
            counts.requested_bytes,
            output.len()
        );
    }
    Ok(())
}
