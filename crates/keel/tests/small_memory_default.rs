//! Full's implicit pipeline expands bounded intrinsics on both backends.
use std::{fs, path::Path, process::Command};

#[test]
fn implicit_full_expands_16_bytes_and_preserves_fallbacks() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("small-memory-default");
    fs::create_dir_all(&dir).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        for length in ["16", "33", "n"] {
            fs::write(dir.join("copy.c"), format!(
                "void copy(char *d, char *s, unsigned long n) {{ __builtin_memcpy(d, s, {length}); }}"
            )).unwrap();
            for tier in ["none", "basic", "full"] {
                let args = if tier == "full" {
                    vec!["-O"]
                } else {
                    vec!["--opt-tier", tier]
                };
                for emit in ["--emit-ir", "asm", "--emit-object"] {
                    let mut command = Command::new(env!("CARGO_BIN_EXE_keel"));
                    command
                        .current_dir(&dir)
                        .args(["cc", "--target", target])
                        .args(&args)
                        .args(if emit == "asm" {
                            vec!["copy.c"]
                        } else if emit == "--emit-object" {
                            vec![emit, "copy.c", "-o", "copy.o"]
                        } else {
                            vec![emit, "copy.c"]
                        });
                    let output = command.output().unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    if emit != "--emit-object" {
                        let text = String::from_utf8(output.stdout).unwrap();
                        assert_eq!(
                            text.contains("memcpy"),
                            tier != "full" || length != "16",
                            "{target} {tier} {length} {emit}: {text}"
                        );
                    }
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ordinary_small_memory_calls_expand_and_preserve_exact_native_footprints() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ordinary-small-memory");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("memory.c"),
        r#"
void *memcpy(void *, const void *, unsigned long);
void *memmove(void *, const void *, unsigned long);
void *memset(void *, int, unsigned long);
int match2(const void *p) {
    union { unsigned short word; unsigned char bytes[2]; } a, b;
    memcpy(&a, p, 2);
    memcpy(&b, ": ", 2);
    return a.word == b.word;
}
void shift7(void *d, const void *s) { memmove(d, s, 7); }
void fill7(void *d, int v) { memset(d, v, 7); }
"#,
    )
    .unwrap();
    let run = |command: &mut Command| {
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{command:?} {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        let assembly = run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args(["cc", "-O", "--target", target, "memory.c"]));
        for name in ["memcpy", "memmove", "memset"] {
            assert!(
                !assembly.contains(name),
                "{target}: ordinary {name} was not expanded"
            );
        }
        fs::write(dir.join("memory.s"), assembly).unwrap();
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args([
                "cc",
                "-O",
                "--target",
                target,
                "--emit-object",
                "memory.c",
                "-o",
                "direct.o",
            ]));
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args(["asm", "--target", target, "memory.s", "-o", "text.o"]));
        let text = |name: &str| {
            use object::{Object, ObjectSection};
            let bytes = fs::read(dir.join(name)).unwrap();
            let file = object::File::parse(bytes.as_slice()).unwrap();
            file.sections()
                .find(|s| matches!(s.name(), Ok(".text" | "__text")))
                .unwrap()
                .data()
                .unwrap()
                .to_vec()
        };
        assert_eq!(
            text("direct.o"),
            text("text.o"),
            "{target}: object routes disagree"
        );
        let native = target == "aarch64-apple-darwin"
            && cfg!(all(target_arch = "aarch64", target_os = "macos"))
            || target == "x86_64-unknown-linux-gnu"
                && cfg!(all(target_arch = "x86_64", target_os = "linux"));
        if native {
            fs::write(
                dir.join("oracle.c"),
                r#"
#include <sys/mman.h>
#include <unistd.h>
extern int match2(const void *);
extern void shift7(void *,const void *);
extern void fill7(void *,int);
int main(void) {
    long page=sysconf(_SC_PAGESIZE);
    unsigned char *p=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
    if(p==MAP_FAILED || mprotect(p+page,page,PROT_NONE))return 1;
    unsigned char *end=p+page;
    const unsigned char cases[]={0, ':', ' ', ';', 0x80, 0xff};
    for(unsigned x=0;x<sizeof(cases);x++)for(unsigned y=0;y<sizeof(cases);y++) {
        end[-2]=cases[x];end[-1]=cases[y];
        if(match2(end-2)!=(cases[x]==':' && cases[y]==' '))return 2;
    }
    for(unsigned i=0;i<8;i++)end[(long)i-8]=i;
    shift7(end-7,end-8);
    for(unsigned i=0;i<7;i++)if(end[(long)i-7]!=i)return 3;
    shift7(end-8,end-7);
    for(unsigned i=0;i<7;i++)if(end[(long)i-8]!=i)return 4;
    end[-8]=0x5a;
    fill7(end-7,0x1234);
    if(end[-8]!=0x5a)return 5;
    for(unsigned i=0;i<7;i++)if(end[(long)i-7]!=0x34)return 6;
    return munmap(p,2*page)!=0;
}
"#,
            )
            .unwrap();
            run(Command::new("clang")
                .current_dir(&dir)
                .args(["-O2", "oracle.c", "direct.o", "-o", "oracle"]));
            run(&mut Command::new(dir.join("oracle")));
        }
    }
}
