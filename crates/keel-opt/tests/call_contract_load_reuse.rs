//! Scoped effects can preserve completed reads without moving or deleting calls.
use keel_ir::{CallContract, InstData, MemoryAccess, MemorySummary, Module};

fn readonly() -> CallContract {
    CallContract {
        memory: MemorySummary {
            argmem: MemoryAccess::Read,
            other: MemoryAccess::Read,
            // Runtime-private writes cannot modify a pointer accessible here.
            inaccessible: MemoryAccess::ReadWrite,
        },
        nofree: true,
        nosync: true,
        nocallback: true,
        preserves_access: true,
        ..Default::default()
    }
}
fn input(indirect: bool) -> Module {
    input_middle(indirect, "")
}
fn input_middle(indirect: bool, middle: &str) -> Module {
    let call = if indirect {
        "v4 = call_indirect v1(v0) : (ptr) -> i64"
    } else {
        "v4 = call @effect(v0)"
    };
    keel_ir::parse(&format!(
        "decl @effect(ptr) -> i64\nfunc @f(ptr, ptr) -> i64 {{\nblock0(v0: ptr, v1: ptr):\n v2 = load.i64 v0\n {call}\n {middle}\n v3 = load.i64 v0\n return v3\n}}"
    ))
    .unwrap()
}
fn attach(m: &mut Module, contract: CallContract, site: bool) {
    let f = &mut m.functions[0];
    let call = f
        .layout_insts()
        .find_map(|(_, i)| {
            matches!(
                f.inst_data(i),
                InstData::Call { .. } | InstData::CallIndirect { .. }
            )
            .then_some(i)
        })
        .unwrap();
    if site {
        f.set_call_contract(call, contract).unwrap();
    } else {
        let InstData::Call { func, .. } = *f.inst_data(call) else {
            panic!("expected direct call")
        };
        f.callee_mut(func).attrs.contract = contract.clone();
        m.decls[0].attrs.contract = contract;
    }
}
fn optimize(m: &mut Module) -> (usize, usize) {
    keel_ir::verify_module(m).unwrap();
    keel_opt::run_passes(m, &["call-load-reuse"]).unwrap();
    keel_ir::verify_module(m).unwrap();
    let f = &m.functions[0];
    let loads = f
        .layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
        .count();
    let calls = f
        .layout_insts()
        .filter(|(_, i)| {
            matches!(
                f.inst_data(*i),
                InstData::Call { .. } | InstData::CallIndirect { .. }
            )
        })
        .count();
    (loads, calls)
}
#[test]
fn declaration_and_site_contracts_preserve_read_and_call() {
    for (indirect, site) in [(false, false), (false, true), (true, true)] {
        let mut m = input(indirect);
        attach(&mut m, readonly(), site);
        assert_eq!(optimize(&mut m), (1, 1), "indirect={indirect}, site={site}");
        assert!(m.to_string().contains("return v2"));
    }
}
#[test]
fn every_lifetime_effect_is_required_and_accessible_writes_invalidate() {
    let mut cases = vec![CallContract::default()];
    for missing in [
        "nofree",
        "nosync",
        "nocallback",
        "preserves_access",
        "argmem",
        "other",
    ] {
        let mut contract = readonly();
        match missing {
            "nofree" => contract.nofree = false,
            "nosync" => contract.nosync = false,
            "nocallback" => contract.nocallback = false,
            "preserves_access" => contract.preserves_access = false,
            "argmem" => contract.memory.argmem = MemoryAccess::Write,
            _ => contract.memory.other = MemoryAccess::ReadWrite,
        }
        cases.push(contract);
    }
    for contract in cases {
        for indirect in [false, true] {
            let mut m = input(indirect);
            attach(&mut m, contract.clone(), true);
            assert_eq!(optimize(&mut m), (2, 1), "{contract:?}");
        }
    }
}
#[test]
fn site_contract_does_not_change_another_call() {
    let mut m = input_middle(false, "v8 = call @effect(v0)");
    attach(&mut m, readonly(), true);
    assert_eq!(optimize(&mut m), (2, 2));
}
#[test]
fn return_twice_calls_still_invalidate_reads() {
    for indirect in [false, true] {
        let mut m = input(indirect);
        let f = &mut m.functions[0];
        let call = f
            .layout_insts()
            .find_map(|(_, i)| {
                matches!(
                    f.inst_data(i),
                    InstData::Call { .. } | InstData::CallIndirect { .. }
                )
                .then_some(i)
            })
            .unwrap();
        match f.inst_data(call).clone() {
            InstData::Call { func, .. } => {
                f.callee_mut(func).sig.returns_twice = true;
                m.decls[0].sig.returns_twice = true;
            }
            InstData::CallIndirect { sig, .. } => {
                let mut signature = f.signature(sig).clone();
                signature.returns_twice = true;
                let replacement = f.import_signature(signature);
                if let InstData::CallIndirect { sig, .. } = f.inst_data_mut(call) {
                    *sig = replacement;
                }
            }
            _ => unreachable!(),
        }
        attach(&mut m, readonly(), true);
        assert_eq!(optimize(&mut m), (2, 1));
    }
}
#[test]
fn unrelated_writes_and_volatile_accesses_remain_barriers() {
    for middle in ["store v4, v0", "v5 = load.i64 volatile v0", "fence seq_cst"] {
        let mut m = input_middle(false, middle);
        attach(&mut m, readonly(), false);
        let expected = if middle.contains("load.i64") { 3 } else { 2 };
        assert_eq!(optimize(&mut m), (expected, 1), "{middle}");
    }
}

#[test]
fn optimized_calls_match_native_readonly_and_writing_oracles() {
    let Some(keel) = std::env::var_os("KEEL_TEST_KEEL") else {
        eprintln!("skipped native execution: set KEEL_TEST_KEEL to the rebuilt Keel executable");
        return;
    };
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return;
    }
    let dir =
        std::env::temp_dir().join(format!("keel-call-contract-native-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    for writes in [false, true] {
        let mut m = input(false);
        let mut contract = readonly();
        if writes {
            contract.memory.argmem = MemoryAccess::ReadWrite;
        }
        attach(&mut m, contract, false);
        assert_eq!(optimize(&mut m), (if writes { 2 } else { 1 }, 1));
        let source = dir.join("input.keel");
        let object = dir.join("output.o");
        let driver = dir.join("driver.c");
        let exe = dir.join("run");
        std::fs::write(&source, m.to_string()).unwrap();
        std::fs::write(&driver,format!("#include <stdint.h>\nextern uint64_t f(uint64_t*, void*);\nuint64_t effect(uint64_t*p){{{}return *p;}}\nint main(void){{uint64_t x=15;return f(&x,0)!={};}}\n",if writes{"++*p;"}else{""},if writes{16}else{15})).unwrap();
        for mut cmd in [
            {
                let mut c = std::process::Command::new(&keel);
                c.args(["compile", "--opt-tier", "none", "--emit-object"])
                    .arg(&source)
                    .arg("-o")
                    .arg(&object);
                c
            },
            {
                let mut c = std::process::Command::new("clang");
                c.arg(&object).arg(&driver).arg("-o").arg(&exe);
                c
            },
            std::process::Command::new(&exe),
        ] {
            let out = cmd.output().unwrap();
            assert!(
                out.status.success(),
                "{cmd:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn permission_changes_preserve_the_later_load_fault() {
    let Some(keel) = std::env::var_os("KEEL_TEST_KEEL") else {
        eprintln!("skipped native execution: set KEEL_TEST_KEEL");
        return;
    };
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return;
    }
    let mut m = input(false);
    let mut contract = readonly();
    contract.preserves_access = false;
    attach(&mut m, contract, true);
    assert_eq!(optimize(&mut m), (2, 1));
    let dir = std::env::temp_dir().join(format!(
        "keel-call-contract-protection-{}",
        std::process::id()
    ));
    std::fs::create_dir(&dir).unwrap();
    let source = dir.join("input.keel");
    let object = dir.join("output.o");
    let driver = dir.join("driver.c");
    let exe = dir.join("run");
    std::fs::write(&source, m.to_string()).unwrap();
    std::fs::write(&driver,"#include <stdint.h>\n#include <stdlib.h>\n#include <unistd.h>\n#include <sys/mman.h>\nextern uint64_t f(uint64_t*,void*);\nuint64_t effect(uint64_t*p){if(mprotect(p,(size_t)getpagesize(),PROT_NONE)) _Exit(3);return 0;}\nint main(void){size_t n=(size_t)getpagesize();uint64_t*p=mmap(0,n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);if(p==MAP_FAILED)return 2;*p=15;return (int)f(p,0);}\n").unwrap();
    let out = std::process::Command::new(keel)
        .args(["compile", "--opt-tier", "none", "--emit-object"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = std::process::Command::new("clang")
        .arg(&object)
        .arg(&driver)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    use std::os::unix::process::ExitStatusExt;
    let status = std::process::Command::new(&exe).status().unwrap();
    assert!(
        matches!(status.signal(), Some(10 | 11)),
        "expected protected-load fault, got {status}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
