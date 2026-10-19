// panic=unwind: catch_unwind, Drop during unwinding, nested panics caught, Result propagation.
use std::cell::RefCell;
use std::hint::black_box;
use std::panic;

thread_local! {
    static LOG: RefCell<String> = const { RefCell::new(String::new()) };
}

fn note(s: &str) {
    LOG.with(|l| l.borrow_mut().push_str(s));
}

struct Guard(&'static str);
impl Drop for Guard {
    fn drop(&mut self) {
        note(self.0);
    }
}

#[inline(never)]
fn leaf(x: u32) -> u32 {
    let _g = Guard("c");
    if x % 3 == 0 {
        panic!("leaf panic {x}");
    }
    let v: Vec<u32> = (0..x).collect();
    v[black_box(x as usize * 7 + 1) % v.len()]
}

#[inline(never)]
fn middle(x: u32) -> u32 {
    let _g = Guard("b");
    leaf(x) * 2
}

fn parse_pair(s: &str) -> Result<(i32, i32), String> {
    let (a, b) = s.split_once(',').ok_or_else(|| format!("no comma in {s:?}"))?;
    let a = a.trim().parse::<i32>().map_err(|e| e.to_string())?;
    let b = b.trim().parse::<i32>().map_err(|e| e.to_string())?;
    Ok((a, b))
}

fn main() {
    panic::set_hook(Box::new(|info| {
        let msg = info.payload().downcast_ref::<String>().map(String::as_str).unwrap_or("?");
        note(&format!("[{msg}]"));
    }));
    let mut results = Vec::new();
    for x in 0..black_box(8u32) {
        let r = panic::catch_unwind(|| {
            let _g = Guard("a");
            middle(x)
        });
        results.push(match r {
            Ok(v) => v as i64,
            Err(_) => -1,
        });
    }
    let parsed: Vec<String> = ["3, 4", "x,1", "7", "-2,9"]
        .iter()
        .map(|s| match parse_pair(s) {
            Ok((a, b)) => format!("{}", a * b),
            Err(e) => format!("E({})", e.len()),
        })
        .collect();
    let log = LOG.with(|l| l.borrow().clone());
    println!("panic_unwind {results:?} {parsed:?} {log}");
}
