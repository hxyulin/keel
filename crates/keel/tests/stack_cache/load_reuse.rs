//! Stores stay observable; exact post-store values forward through CFG chains.
use super::run;
use std::{fs, path::Path, process::Command};
#[test]
fn native_post_store_reads_preserve_arbitrary_alias_observations() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("store-load-chain");
    fs::create_dir_all(&dir).unwrap();
    let source = "func @update(ptr, ptr, i64) -> i64 {
block0(v0: ptr, v1: ptr, v2: i64):
 v3 = load.i64 v0
 store v2, v0
 jump block1
block1:
 v4 = load.i64 v1
 v5 = load.i64 v0
 v6 = xor v3, v4
 v7 = iadd v6, v5
 return v7
}";
    fs::write(dir.join("before.keel"), source).unwrap();
    fs::write(dir.join("oracle.c"),"#include <stdint.h>\n#include <assert.h>\nextern uint64_t update(uint64_t*,uint64_t*,uint64_t);\nint main(void){uint64_t state=1;for(unsigned i=0;i<10000;i++){state=state*6364136223846793005ULL+1;uint64_t old=state,y=state^0x123456789abcdef0ULL,z=~state;for(int alias=0;alias<2;alias++){uint64_t x=old;uint64_t result=update(&x,alias?&x:&y,z);assert(result==((old^(alias?z:y))+z));assert(x==z);assert(y==(state^0x123456789abcdef0ULL));}}}\n").unwrap();
    run(Command::new("clang")
        .current_dir(&dir)
        .args(["-O2", "-c", "oracle.c", "-o", "oracle.o"]));
    let bin = env!("CARGO_BIN_EXE_keel");
    run(Command::new(bin).current_dir(&dir).args([
        "opt",
        "--verify-each",
        "-p",
        "load-reuse",
        "before.keel",
        "-o",
        "after.keel",
    ]));
    let text = fs::read_to_string(dir.join("after.keel")).unwrap();
    assert_eq!(text.matches(" = load.").count(), 2);
    assert_eq!(text.matches("    store ").count(), 1);
    for src in ["before.keel", "after.keel"] {
        for isel in ["none", "all"] {
            for allocation in ["fast", "full"] {
                run(Command::new(bin).current_dir(&dir).args([
                    "compile",
                    "--isel",
                    isel,
                    "--regalloc",
                    allocation,
                    "--emit-object",
                    src,
                    "-o",
                    "module.o",
                ]));
                run(Command::new("clang")
                    .current_dir(&dir)
                    .args(["oracle.o", "module.o", "-o", "oracle"]));
                run(&mut Command::new(dir.join("oracle")));
            }
        }
    }
}
