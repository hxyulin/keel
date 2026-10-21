//! Standalone probe; deliberately does not register or schedule the prototype.
#[path = "../../keel-opt/src/aggregate_return.rs"]
mod aggregate_return;
use keel_ir::{Function, Inst, InstData, StackSlot, ValueDef};
fn call_home(f: &Function, i: Inst) -> Option<StackSlot> {
    let InstData::Call { args, .. } = f.inst_data(i) else {
        return None;
    };
    let ValueDef::Result(addr, _) = f.value_def(f.resolve(*args.first()?)) else {
        return None;
    };
    match f.inst_data(addr) {
        InstData::StackAddr { slot } => Some(*slot),
        _ => None,
    }
}
fn memory(m: &keel_ir::Module) -> (usize, usize) {
    let mut counts = (0, 0);
    for f in &m.functions {
        for (_, i) in f.layout_insts() {
            match f.inst_data(i) {
                InstData::Load { .. } => counts.0 += 1,
                InstData::Store { .. } => counts.1 += 1,
                _ => {}
            }
        }
    }
    counts
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "INPUT.keel OUTPUT.keel");
    let input = std::fs::read_to_string(&args[1]).unwrap();
    let mut m = keel_ir::parse(&input).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let before: Vec<_> = m.functions.iter().map(ToString::to_string).collect();
    let homes: Vec<Vec<_>> = m
        .functions
        .iter()
        .map(|f| {
            f.layout_insts()
                .filter_map(|(_, i)| call_home(f, i).map(|slot| (i, slot)))
                .collect()
        })
        .collect();
    let old_memory = memory(&m);
    let copies = aggregate_return::run(&mut m);
    keel_ir::verify_module(&m).unwrap();
    let new_memory = memory(&m);
    let changed: Vec<_> = m
        .functions
        .iter()
        .zip(&before)
        .filter(|(f, old)| f.to_string() != **old)
        .map(|(f, _)| format!("\"{}\"", f.name))
        .collect();
    let mut sites = Vec::new();
    for (f, calls) in m.functions.iter().zip(homes) {
        for (call, old_slot) in calls {
            if let Some(new_slot) = call_home(f, call)
                && new_slot != old_slot
            {
                let InstData::Call { func, .. } = f.inst_data(call) else {
                    unreachable!()
                };
                sites.push(format!("{{\"function\":\"{}\",\"callee\":\"{}\",\"source_slot\":\"{old_slot}\",\"destination_slot\":\"{new_slot}\"}}", f.name, f.callee(*func).name));
            }
        }
    }
    std::fs::write(&args[2], m.to_string()).unwrap();
    println!(
        "{{\"copies\":{copies},\"loads_removed\":{},\"stores_removed\":{},\"changed_functions\":[{}],\"sites\":[{}]}}",
        old_memory.0 - new_memory.0,
        old_memory.1 - new_memory.1,
        changed.join(","),
        sites.join(",")
    );
}
