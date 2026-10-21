target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"

; A loop with parallel PHIs, a textually forward dominating definition,
; duplicate branch edges, duplicate switch edges and unreachable CFG.
define i64 @cfg(i64 %n, i64 %x) {
entry:
  br label %setup
loop:
  %i = phi i64 [ 0, %setup ], [ %next, %body ]
  %a = phi i64 [ %seed, %setup ], [ %b, %body ]
  %b = phi i64 [ %x, %setup ], [ %mix, %body ]
  %done = icmp uge i64 %i, %n
  br i1 %done, label %dispatch, label %body
body:
  %mix = xor i64 %a, %i
  %next = add i64 %i, 1
  br label %loop
setup:
  %seed = add i64 %x, 17
  br label %loop
dispatch:
  %tag = and i64 %a, 3
  switch i64 %tag, label %exit [ i64 0, label %same
                               i64 1, label %same
                               i64 2, label %exit ]
same:
  %s = phi i64 [ %b, %dispatch ], [ %b, %dispatch ]
  %test = icmp eq i64 %s, 0
  br i1 %test, label %exit, label %exit
exit:
  %r = phi i64 [ %a, %dispatch ], [ %a, %dispatch ], [ %s, %same ], [ %s, %same ]
  ret i64 %r
dead:
  br label %dead
}

; Struct padding, nested array strides, negative/narrow indices, fixed stack.
define i64 @memory(i64 %x, i32 %index) {
entry:
  %slot = alloca { i8, [4 x i64], i16 }, align 16
  %base = getelementptr { i8, [4 x i64], i16 }, ptr %slot, i64 0, i32 1, i64 0
  %last = getelementptr inbounds i64, ptr %base, i32 3
  %back = getelementptr i64, ptr %last, i8 -3
  store volatile i64 %x, ptr %back, align 8
  %p = getelementptr { i8, [4 x i64], i16 }, ptr %slot, i64 0, i32 1, i32 %index
  store i64 19, ptr %p, align 8
  %v = load volatile i64, ptr %base, align 8
  %out = add i64 %v, %x
  ret i64 %out
}

; i1 PHI normalization and switch case spelling.
define i32 @bool_cfg(i32 %x) {
  %b = trunc i32 %x to i1
  br i1 %b, label %yes, label %no
yes:
  br label %join
no:
  br label %join
join:
  %p = phi i1 [ true, %yes ], [ false, %no ]
  switch i1 %p, label %zero [ i1 true, label %one ]
zero:
  ret i32 0
one:
  ret i32 1
}

; Mixed signed switch boundaries and constant PHI arguments.
define i64 @boundaries(i8 %tag, ptr %p) {
  switch i8 %tag, label %other [ i8 -128, label %low
                              i8 127, label %high ]
low:
  br label %join
high:
  %q = getelementptr i8, ptr %p, i64 1
  br label %join
other:
  br label %join
join:
  %r = phi ptr [ %p, %low ], [ %q, %high ], [ %p, %other ]
  %k = phi i64 [ -9223372036854775808, %low ], [ 9223372036854775807, %high ], [ 0, %other ]
  %v = load i8, ptr %r, align 1
  %w = zext i8 %v to i64
  %out = xor i64 %k, %w
  ret i64 %out
}

; An irreducible two-entry cycle, bounded by a counter on each backedge.
define i64 @irreducible(i64 %n, i64 %x) {
  %c = icmp eq i64 %x, 0
  br i1 %c, label %a, label %b
a:
  %ai = phi i64 [ %n, %0 ], [ %bn, %b ]
  %az = icmp eq i64 %ai, 0
  %an = sub i64 %ai, 1
  br i1 %az, label %exit, label %b
b:
  %bi = phi i64 [ %n, %0 ], [ %an, %a ]
  %bz = icmp eq i64 %bi, 0
  %bn = sub i64 %bi, 1
  br i1 %bz, label %exit, label %a
exit:
  %out = phi i64 [ 11, %a ], [ 23, %b ]
  ret i64 %out
}

; Exercise ABI padding and packed layout against caller-initialized bytes.
define i64 @layout(ptr %p, i32 %i) {
  %a = getelementptr { i8, i32, [2 x i16] }, ptr %p, i32 %i, i32 2, i64 1
  %b = getelementptr <{ i8, i32, [2 x i16] }>, ptr %p, i32 %i, i32 2, i64 1
  %av = load i16, ptr %a, align 1
  %bv = load i16, ptr %b, align 1
  %aw = zext i16 %av to i64
  %bw = zext i16 %bv to i64
  %out = add i64 %aw, %bw
  ret i64 %out
}

; Fixed alloca count and zero-index GEP; symbol must survive SSA renaming.
define i64 @v1(i64 %x) {
  %a = alloca i64, i32 2, align 8
  %p = getelementptr i64, ptr %a
  %q = getelementptr i64, ptr %a, i64 1
  store i64 %x, ptr %p, align 8
  store i64 37, ptr %q, align 8
  %v = load i64, ptr %p, align 8
  %w = load i64, ptr %q, align 8
  %r = add i64 %v, %w
  ret i64 %r
}
