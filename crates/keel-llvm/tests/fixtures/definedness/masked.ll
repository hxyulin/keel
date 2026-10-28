target triple = "arm64-apple-macosx11.0.0"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"

; Overflow is deferred poison, which is not selected when take is zero.
define i64 @add_masked(i64 %x, i64 %take) {
entry:
  %sum = add nsw i64 %x, 1
  %condition = icmp ne i64 %take, 0
  %result = select i1 %condition, i64 %sum, i64 7
  ret i64 %result
}

; The incoming edge, rather than block order, selects the observed value.
define i64 @phi_masked(i64 %x, i64 %take) {
entry:
  %sum = add nsw i64 %x, 1
  %condition = icmp ne i64 %take, 0
  br i1 %condition, label %use, label %skip
use:
  br label %join
skip:
  br label %join
join:
  %result = phi i64 [ %sum, %use ], [ 7, %skip ]
  ret i64 %result
}

; nonnull without noundef may poison p. Selecting fallback masks that poison
; before the load, so null with take zero is a defined source execution.
define i64 @nonnull_masked(ptr nonnull %p, i64 %take) {
entry:
  %fallback = alloca i64, align 8
  store i64 91, ptr %fallback, align 8
  %condition = icmp ne i64 %take, 0
  %chosen = select i1 %condition, ptr %p, ptr %fallback
  %result = load i64, ptr %chosen, align 8
  ret i64 %result
}

; An oversized LLVM shift is poison, but remains unobserved on the false arm.
define i64 @shift_masked(i64 %x, i64 %amount, i64 %take) {
entry:
  %shifted = shl i64 %x, %amount
  %condition = icmp ne i64 %take, 0
  %result = select i1 %condition, i64 %shifted, i64 7
  ret i64 %result
}

; Keep this call inside LLVM IR so a C frontend cannot add a call-site noundef
; contract to the null argument. The wrapper's observed result is defined.
define i64 @nonnull_null_masked() {
entry:
  %result = call i64 @nonnull_masked(ptr null, i64 0)
  ret i64 %result
}
