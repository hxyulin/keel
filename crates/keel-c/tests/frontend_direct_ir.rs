//! The IR the frontend emits directly: operations on literals, null
//! pointers, constant address steps, truth tests and discarded values cost
//! no instructions of their own, while everything with an effect, a trap or
//! a type-dependent result is still built. The structural tests pin the
//! shapes; the program at the end compares behavior at the boundaries with
//! clang, built as is, optimized and with debug info.

use std::path::Path;
use std::process::Command;

fn ir(src: &str, debug_info: bool) -> String {
    let module = keel_c::compile(
        src,
        "direct.c",
        keel_c::Options {
            debug_info,
            ..Default::default()
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module.to_string()
}

/// The printed body of function `name`.
fn body<'a>(ir: &'a str, name: &str) -> &'a str {
    let start = ir
        .find(&format!("func @{name}("))
        .unwrap_or_else(|| panic!("no function {name} in\n{ir}"));
    let end = ir[start..].find("\n}").unwrap();
    &ir[start..start + end]
}

/// Instructions of opcode `op` (`iconst.i64 0` style prefixes included).
fn count(body: &str, op: &str) -> usize {
    body.lines()
        .filter(|l| {
            let l = l.trim();
            let inst = l.split_once(" = ").map_or(l, |(_, rhs)| rhs);
            inst == op || inst.starts_with(&format!("{op} ")) || inst.starts_with(&format!("{op}."))
        })
        .count()
}

/// Instructions: the indented lines after the first block header (before
/// it are stack slots and debug declarations).
fn insts(body: &str) -> usize {
    body.lines()
        .skip_while(|l| !l.starts_with("block"))
        .filter(|l| l.starts_with("    "))
        .count()
}

#[test]
fn literal_operations_are_one_constant() {
    let src = "int f(void) { return ((3) | ((1) << 4)) + -1 + ~0 + !0 + (int)(char)300; }\n\
               unsigned long g(void) { return sizeof(int) * 3 + (0u - 1u) / 2u; }";
    for debug in [false, true] {
        let ir = ir(src, debug);
        for name in ["f", "g"] {
            let f = body(&ir, name);
            assert_eq!(count(f, "iconst"), 1, "{f}");
            assert_eq!(insts(f), 2, "{f}");
        }
        // 19 + -1 + -1 + 1 + 44, and 12 + 2147483647.
        assert!(body(&ir, "f").contains("iconst.i32 62"), "{ir}");
        assert!(body(&ir, "g").contains("iconst.i64 2147483659"), "{ir}");
    }
}

#[test]
fn deep_literal_chains_and_literal_conditionals_fold() {
    let src = "int flags(void) { return 1|2|4|8|16|32|64|128|256|512|1024|2048|4096|8192; }\n\
               long width(void) { return (sizeof(int) < sizeof(long) ? 64 : 32) - 1; }";
    let ir = ir(src, false);
    for name in ["flags", "width"] {
        let f = body(&ir, name);
        assert_eq!(insts(f), 2, "{f}");
    }
    assert!(body(&ir, "flags").contains("iconst.i32 16383"), "{ir}");
    assert!(body(&ir, "width").contains("iconst.i64 63"), "{ir}");
}

#[test]
fn identity_operands_emit_no_operation() {
    let src = "unsigned long bytes(unsigned long n) { return n * sizeof(char); }\n\
               int shifts(int x) { return ((x << 0) >> 0) + 0 - 0 | 0; }\n\
               unsigned masks(unsigned x) { return 0xffffffffu & x & ~0u; }\n\
               int left(int x) { return 0 + (1 * x) ^ 0; }\n\
               int kept(int x) { return (x & 0xffff) + (0 - x) + x % 1 + (0 >> x); }";
    for debug in [false, true] {
        let ir = ir(src, debug);
        for name in ["bytes", "shifts", "masks", "left"] {
            let f = body(&ir, name);
            for op in [
                "imul", "shl", "sshr", "iadd", "isub", "or", "and", "xor", "iconst",
            ] {
                assert_eq!(count(f, op), 0, "{name}: {f}");
            }
        }
        let kept = body(&ir, "kept");
        for op in ["and", "isub", "srem", "sshr"] {
            assert_eq!(count(kept, op), 1, "{op}: {kept}");
        }
    }
}

#[test]
fn bit_fields_at_the_ends_of_their_unit_shift_once() {
    let src = "struct bf { unsigned low : 3, mid : 5, : 16, top : 8; };\n\
               void set_low(struct bf *p, unsigned v) { p->low = v; }\n\
               void set_mid(struct bf *p, unsigned v) { p->mid = v; }\n\
               unsigned get_low(struct bf *p) { return p->low; }\n\
               unsigned get_top(struct bf *p) { return p->top; }";
    let ir = ir(src, false);
    assert_eq!(count(body(&ir, "set_low"), "shl"), 0, "{ir}");
    assert_eq!(count(body(&ir, "set_mid"), "shl"), 1, "{ir}");
    assert_eq!(count(body(&ir, "get_low"), "shl"), 1, "{ir}");
    let top = body(&ir, "get_top");
    assert_eq!(count(top, "shl"), 0, "{top}");
    assert_eq!(count(top, "ushr"), 1, "{top}");
}

#[test]
fn trapping_and_out_of_range_literal_operations_are_built() {
    let src = "int div0(void) { return 1 / 0; }\n\
               int rem0(void) { return 1 % 0; }\n\
               unsigned udiv0(void) { return 1u / 0u; }\n\
               int minus1(void) { return (-2147483647 - 1) / -1; }\n\
               long lminus1(void) { return (-9223372036854775807L - 1) % -1L; }\n\
               int wide(void) { return 1 << 32; }\n\
               int negative(void) { return 1 >> -1; }";
    let ir = ir(src, false);
    for (name, op) in [
        ("div0", "sdiv"),
        ("rem0", "srem"),
        ("udiv0", "udiv"),
        ("minus1", "sdiv"),
        ("lminus1", "srem"),
        ("wide", "shl"),
        ("negative", "sshr"),
    ] {
        assert_eq!(count(body(&ir, name), op), 1, "{name}: {ir}");
    }
}

#[test]
fn integer_constants_made_pointers_or_floats_convert_once() {
    let src = "void *null(void) { return (void *)0; }\n\
               char *odd(void) { return (char *)-16; }\n\
               int pos(double *p) { return *p > 0; }\n\
               float big(void) { return (float)18446744073709551615ULL; }";
    for debug in [false, true] {
        let ir = ir(src, debug);
        let null = body(&ir, "null");
        assert_eq!(count(null, "sext"), 0, "{null}");
        assert!(null.contains("iconst.i64 0"), "{null}");
        assert_eq!(count(null, "inttoptr"), 1, "{null}");
        assert!(body(&ir, "odd").contains("iconst.i64 -16"), "{ir}");
        let pos = body(&ir, "pos");
        assert_eq!(count(pos, "sitofp"), 0, "{pos}");
        assert_eq!(count(pos, "fconst"), 1, "{pos}");
        let big = body(&ir, "big");
        assert_eq!(count(big, "uitofp"), 0, "{big}");
        assert_eq!(count(big, "fconst"), 1, "{big}");
    }
}

#[test]
fn constant_address_steps_add_one_offset() {
    let src = "struct in { char c; int b; long l; };\n\
               struct out { int a; struct in in; struct in arr[3]; };\n\
               int member(struct out *p) { return p->in.b; }\n\
               long element(struct out *p) { return p[2].arr[1].l; }\n\
               char negative(struct in *q) { return q[-1].c; }\n\
               char cast(struct out *p) { return ((char *)p + 8)[3]; }\n\
               int first(int *p) { return p[0] + *(p + 0); }\n\
               int runtime(struct out *p, long i) { return p->arr[i].b; }";
    for debug in [false, true] {
        let ir = ir(src, debug);
        for name in ["member", "element", "negative", "cast"] {
            assert_eq!(count(body(&ir, name), "ptradd"), 1, "{name}: {ir}");
        }
        assert_eq!(count(body(&ir, "first"), "ptradd"), 0, "{ir}");
        // Offset of arr, then the scaled index, then the member.
        assert_eq!(count(body(&ir, "runtime"), "ptradd"), 3, "{ir}");
    }
    let ir = ir(src, false);
    // 2 * 72 + 24 + 1 * 16 + 8, and -16.
    assert!(body(&ir, "element").contains("iconst.i64 192"), "{ir}");
    assert!(body(&ir, "negative").contains("iconst.i64 -16"), "{ir}");
    assert!(body(&ir, "cast").contains("iconst.i64 11"), "{ir}");
}

#[test]
fn truth_tests_branch_on_the_comparison() {
    let src = "int g(void);\n\
               int likely(int a, int b) { if (__builtin_expect(((a == b) != 0), 1)) return g(); return 0; }\n\
               int nested(int a, int b) { if (((long)(a < b)) == 0) return g(); return 1; }\n\
               int value(unsigned a, unsigned b) { return (a < b) != 0; }\n\
               int not_int(int *p, int *q) { return !(p < q); }";
    for debug in [false, true] {
        let ir = ir(src, debug);
        for name in ["likely", "nested"] {
            let f = body(&ir, name);
            assert_eq!(count(f, "icmp"), 1, "{f}");
            assert_eq!(count(f, "uext") + count(f, "sext"), 0, "{f}");
        }
        let value = body(&ir, "value");
        assert_eq!(count(value, "icmp"), 1, "{value}");
        assert!(value.contains("icmp.ult"), "{value}");
        let not = body(&ir, "not_int");
        assert_eq!(count(not, "icmp"), 1, "{not}");
        assert!(not.contains("icmp.uge"), "{not}");
    }
}

#[test]
fn float_and_narrowing_truth_tests_keep_their_conversions() {
    let src = "int not_float(double a, double b) { return !(a < b); }\n\
               int narrow(long x) { if ((int)x) return 1; return 0; }\n\
               int byte(int x) { return (_Bool)(char)x; }";
    let ir = ir(src, false);
    let not = body(&ir, "not_float");
    // `!(NaN < b)` is 1: no `fcmp.ge`.
    assert!(not.contains("fcmp.lt") && not.contains("icmp.eq"), "{not}");
    assert_eq!(count(body(&ir, "narrow"), "trunc"), 1, "{ir}");
    assert_eq!(count(body(&ir, "byte"), "trunc"), 1, "{ir}");
}

#[test]
fn literal_conditions_keep_both_edges_and_select_conditional_arms() {
    let src = "int f(double), g(double);\n\
               int pick(double x) { return sizeof(x) == sizeof(float) ? f(x) : g(x); }\n\
               int labeled(double x) { return 0 ? ({ f(x); }) : g(x); }\n\
               void side(int *p) { int r = 1 ? (*p += 1) : (*p += 100); (void)r; }\n\
               int branch(void) { if (sizeof(long) == 8) return 1; return 2; }";
    for debug in [false, true] {
        let ir = ir(src, debug);
        let pick = body(&ir, "pick");
        assert_eq!(count(pick, "call"), 1, "{pick}");
        assert!(pick.contains("@g("), "{pick}");
        assert_eq!(count(pick, "brif"), 0, "{pick}");
        // A statement expression could hold a label: built as before.
        assert_eq!(count(body(&ir, "labeled"), "call"), 2, "{ir}");
        assert_eq!(count(body(&ir, "side"), "iadd"), 1, "{ir}");
        // Both edges of the literal test remain; only the comparison goes.
        let branch = body(&ir, "branch");
        assert_eq!(count(branch, "brif"), 1, "{branch}");
        assert_eq!(count(branch, "icmp"), 0, "{branch}");
    }
}

#[test]
fn discarded_values_without_effects_emit_nothing() {
    let src = "int g(void);\n\
               void inert(int a) { (void)0; (void)a; 0, (void)(long)a; 1.5; }\n\
               void calls(void) { (void)(g(), 0); }\n\
               void reads(volatile int *p) { (void)*p; }\n\
               void atomic_reads(int *p) { (void)__atomic_load_n(p, 5); __atomic_load_n(p, 5); }\n\
               long expect(long x) { return __builtin_expect(x, g()); }\n\
               long literal(long x) { return __builtin_expect(x, 1); }";
    for debug in [false, true] {
        let ir = ir(src, debug);
        let inert = body(&ir, "inert");
        if debug {
            // The parameter's stack copy; nothing for the statements.
            assert_eq!(count(inert, "load"), 0, "{inert}");
        } else {
            assert_eq!(insts(inert), 1, "{inert}");
        }
        assert_eq!(count(body(&ir, "calls"), "call"), 1, "{ir}");
        assert!(body(&ir, "reads").contains("load.i32 volatile"), "{ir}");
        assert_eq!(count(body(&ir, "atomic_reads"), "atomic_load"), 2, "{ir}");
        assert_eq!(count(body(&ir, "expect"), "call"), 1, "{ir}");
        assert_eq!(count(body(&ir, "literal"), "iconst"), 0, "{ir}");
    }
}

#[test]
fn declarations_run_once_need_no_zero_but_repeated_ones_keep_it() {
    let src = "int g(int);\n\
               int once(int c) { int x; x = g(c); return x; }\n\
               int looped(int n) { int s = 0; for (int i = 0; i < n; i++) { int x; if (i & 1) x = i; s += x; } return s; }\n\
               int relabeled(int n) { again: { int x; if (n) x = 7; n = g(x); } if (n > 0) goto again; return n; }";
    let ir = ir(src, false);
    assert_eq!(count(body(&ir, "once"), "iconst"), 0, "{ir}");
    // A zero is defined after the loop starts, not only on entry.
    for name in ["looped", "relabeled"] {
        let f = body(&ir, name);
        let later = f.split_once("\nblock1").unwrap().1;
        assert!(later.contains("iconst.i32 0"), "{name}: {f}");
    }
}

// ---- behavior, compared with clang ----

const PROGRAM: &str = r#"
int printf(const char *, ...);
static int calls;
static int bump(void) { return ++calls; }
volatile int vz;
volatile double dz;
struct in { char c; int b; long l; };
struct out { int a; struct in in; struct in arr[3]; };
struct bf { unsigned low : 3, mid : 5, : 16, top : 8; int slow : 4, : 20, stop : 8; };

int main(void) {
    printf("%d %d %d\n", (signed char)200, (unsigned char)-1, (short)70000);
    printf("%u %u %u\n", 0u - 1u, (unsigned)-1 >> 31, 1u << 31);
    printf("%d %d %u\n", -7 / 2, -7 % 2, 7u / 2u);
    printf("%d %d %d\n", -8 >> 1, (int)(0x80000000u >> 31), -2147483647 - 1 < 0);
    printf("%lld %llu\n", -9223372036854775807LL - 1, 18446744073709551615ULL / 3);
    printf("%d %d %d\n", 1u > -1, -1 < 0u, (unsigned char)255 == -1);
    printf("%d %lu %d %d\n", (unsigned short)65535 + 1, sizeof(int) * 3, !5, !0);
    printf("%d %d\n", ~0u == 4294967295u, (long)~0 == -1L);
    printf("%d %ld\n", 1|2|4|8|16|32|64|128|256|512|1024|2048|4096|8192,
           (sizeof(int) < sizeof(long) ? 64 : 32) - 1L);
    printf("%.1f %.1f\n", (double)9007199254740993LL, (float)16777217);
    printf("%.1f %.1f\n", (double)18446744073709551615ULL, (float)18446744073709551615ULL);
    printf("%.1f %.1f\n", (double)-1, (float)(unsigned char)-1);

    double nan = dz / dz;
    printf("%d %d %d %d\n", !(nan < 1.0), !(nan >= 1.0), (nan < 1.0) == 0, (_Bool)nan);
    printf("%d %d %d\n", (_Bool)-0.0, (_Bool)0.5, (_Bool)(dz + 0.25));

    long big = 0x100000000L;
    int x256 = 256 + vz;
    printf("%d %d %d\n", big ? 1 : 0, (int)big ? 1 : 0, (_Bool)big);
    if ((char)x256) printf("char wrong\n"); else printf("char narrow\n");
    if (__builtin_expect((int)big, 1)) printf("int wrong\n"); else printf("int narrow\n");
    if (__builtin_expect(big != 0, 0)) printf("wide\n");
    if (__builtin_expect(big, bump())) printf("calls %d\n", calls);
    if (!(long)(char)x256) printf("not narrow\n");
    short s16 = 0x10000 + vz;
    unsigned char u8 = 0x1ff + vz;
    _Bool b1 = 0x100 + vz;
    int steps = 0;
    for (unsigned char w = 0x180 + vz; w; w >>= 1) steps++;
    printf("%d %d %d %d\n", s16 ? 1 : 2, u8 ? 1 : 2, b1 ? 1 : 2, steps);
    if (s16) printf("short wrong\n");
    if (__builtin_expect(u8 != 0, 1)) printf("byte set\n");

    int a = 3 + vz, b = 4 + vz;
    unsigned ua = 3 + vz, ub = 0xffffffffu + vz;
    printf("%d %d %d %d\n", (a < b) != 0, (a < b) == 0, !(ua < ub), ((long)(a == b)) != 0);
    printf("%d %d %d\n", !(a >= b), !(ua > ub), !!(a - 3));
    int amin = -2147483647 - 1 + vz;
    printf("%d %d %d %d %d\n", amin * 1, amin / 1, 0 - a, (a << 0) | 0, a % 1);
    printf("%u %d %ld\n", ub & ~0u, (unsigned char)ub & -1, (long)sizeof(char) * big);
    int *p0 = 0, *p1 = &a;
    printf("%d %d %d\n", !(p0 < p1), (p0 == 0) == 0, p1 != (void *)0);

    struct out o = {0};
    o.in.b = 5 + vz;
    o.arr[2].l = 6 + vz;
    o.arr[1].c = 7 + vz;
    struct in *q = &o.arr[2];
    printf("%d %ld %d\n", (&o)->in.b, q[0].l, q[-1].c);
    char *cp = (char *)&o.arr[1] + 0;
    printf("%d %ld\n", cp[0], (char *)&o.arr[2].l - (char *)&o);
    int arr[5] = {1, 2, 3, 4, 5};
    int *e = arr + 4;
    printf("%d %d %d %d\n", e[-4], *(e - 2), (arr + 1)[1 + 1], arr[-1 + 1]);

    struct bf f = {0};
    f.low = 13 + vz;
    f.mid = 31 + vz;
    f.top = 0x1ff + vz;
    f.slow = -3 + vz;
    f.stop = 200 + vz;
    printf("%u %u %u %d %d\n", f.low, f.mid, f.top, f.slow, f.stop);
    f.low = 0;
    f.stop = -1;
    printf("%u %u %u %d %d\n", f.low, f.mid, f.top, f.slow, f.stop);

    int k = 0;
    int r = sizeof(long) == 8 ? (k += 1, 10) : (k += 100, 20);
    int r2 = 0 ? bump() : 5;
    printf("%d %d %d %d\n", r, k, r2, calls);
    (void)bump();
    (void)(bump(), 0);
    0, bump();
    (void)vz;
    volatile int lv = 3;
    (void)lv;
    printf("%d\n", calls);

    int n = 0;
    do { n++; } while (0);
    while (1) { if (++n > 3) break; }
    for (; 0;) n += 100;
    if (sizeof(int) == 4) printf("int4 %d\n", n); else printf("other\n");
    return (a < b) + 40;
}
"#;

fn build(src: &Path, exe: &Path, optimize: bool, debug_info: bool) {
    let text = std::fs::read_to_string(src).unwrap();
    let opts = keel_c::Options {
        debug_info,
        ..Default::default()
    };
    let mut module = keel_c::compile(&text, src.to_str().unwrap(), opts).unwrap();
    keel_ir::verify_module(&module).unwrap();
    if optimize {
        keel_opt::optimize(&mut module).unwrap();
    }
    let asm = keel_codegen::compile(&module, opts.target).unwrap();
    let asm_path = exe.with_extension("s");
    std::fs::write(&asm_path, asm).unwrap();
    let out = Command::new("clang")
        .args(debug_info.then_some("-g"))
        .arg("-o")
        .arg(exe)
        .arg(&asm_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "clang: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn output(exe: &Path) -> (Option<i32>, String) {
    let out = Command::new(exe).output().unwrap();
    (out.status.code(), String::from_utf8(out.stdout).unwrap())
}

#[test]
fn boundary_program_matches_clang() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("frontend-direct-ir");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("boundary.c");
    std::fs::write(&src, PROGRAM).unwrap();
    let reference = dir.join("boundary-clang");
    let out = Command::new("clang")
        .args(["-std=gnu11", "-w", "-O0", "-o"])
        .arg(&reference)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let want = output(&reference);
    assert!(
        want.1.contains("calls 1\n") && want.1.contains("int4 4\n"),
        "{}",
        want.1
    );
    for (name, optimize, debug) in [
        ("keel", false, false),
        ("keel-opt", true, false),
        ("keel-g", false, true),
    ] {
        let exe = dir.join(name);
        build(&src, &exe, optimize, debug);
        assert_eq!(output(&exe), want, "{name}");
    }
}

/// An uninitialized register variable reads as zero where no assignment
/// reaches it, also on later iterations of a loop around its declaration
/// and after a backward `goto`. (Not a clang comparison: C leaves it
/// indeterminate; Keel keeps its result fixed.)
#[test]
fn repeated_declarations_still_start_at_zero() {
    let program = r#"
int printf(const char *, ...);
volatile int vz;
int main(void) {
    int out = 0;
    for (int i = 0; i < 4; i++) {
        int x;
        if (i == vz) x = 5;
        out = out * 10 + x;
    }
    int n = 0, back = 0;
again:
    {
        int y;
        if (n == vz) y = 7;
        back = back * 10 + y;
    }
    if (++n < 3) goto again;
    int z;
    if (vz) z = 9;
    printf("%d %d %d\n", out, back, z);
    return 0;
}
"#;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("frontend-direct-ir");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("zero.c");
    std::fs::write(&src, program).unwrap();
    for (name, optimize) in [("zero", false), ("zero-opt", true)] {
        let exe = dir.join(name);
        build(&src, &exe, optimize, false);
        assert_eq!(output(&exe), (Some(0), "5000 700 0\n".into()), "{name}");
    }
}
