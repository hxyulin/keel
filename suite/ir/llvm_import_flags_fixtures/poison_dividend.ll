; sdiv MIN/-1 with concrete operands is immediate UB, not masked poison.
; These dividends instead become poison upstream on the masked executions.
; Wrapping refinement produces MIN: guard sdiv to avoid introducing a trap.
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
define i64 @masked_add_div(i64 %x, i64 %take) {
  %p = add nsw i64 %x, 1
  %q = sdiv i64 %p, -1
  %c = icmp ne i64 %take, 0
  %r = select i1 %c, i64 %q, i64 73
  ret i64 %r
}
define i64 @unused_add_div(i64 %x) {
  %p = add nsw i64 %x, 1
  %q = sdiv i64 %p, -1
  ret i64 73
}
define i64 @masked_add_rem(i64 %x, i64 %take) {
  %p = add nsw i64 %x, 1
  %q = srem i64 %p, -1
  %c = icmp ne i64 %take, 0
  %r = select i1 %c, i64 %q, i64 73
  ret i64 %r
}
define i64 @masked_exact_div(i64 %x, i64 %take) {
  %p = lshr exact i64 %x, 1
  %q = shl i64 %p, 63
  %r = sdiv exact i64 %q, -1
  %c = icmp ne i64 %take, 0
  %v = select i1 %c, i64 %r, i64 73
  ret i64 %v
}
; Zero-divisor traps remain at the source's immediate-UB boundary.
define i64 @zero_divisor(i64 %x, i64 %y) {
  %r = sdiv i64 %x, %y
  ret i64 %r
}
