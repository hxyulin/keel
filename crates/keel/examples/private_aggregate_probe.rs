use std::fs;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut m = keel_ir::parse(&fs::read_to_string(&args[1]).unwrap()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let old: Vec<_> = m
        .functions
        .iter()
        .map(|f| (f.name.clone(), f.sig.clone()))
        .collect();
    let changed = keel_opt::private_aggregate::run(&mut m);
    keel_ir::verify_module(&m).unwrap();
    let changed_names = m
        .functions
        .iter()
        .zip(&old)
        .filter(|(f, (_, sig))| f.sig != *sig)
        .map(|(f, _)| format!("\"{}\"", f.name))
        .collect::<Vec<_>>()
        .join(",");
    if args.len() > 3 {
        // Static cleanup of the two requested retained helpers only.
        for f in &mut m.functions {
            if ["JS_FreeValue", "JS_CallInternal"].contains(&f.name.as_str()) {
                for name in ["stack-forward", "stack-promote", "dce"] {
                    keel_opt::find_pass(name).unwrap().run(f);
                }
            }
        }
        keel_ir::verify_module(&m).unwrap();
    }
    println!("{{\"definitions\":{changed},\"changed\":[{changed_names}]}}");
    fs::write(&args[2], m.to_string()).unwrap();
}
