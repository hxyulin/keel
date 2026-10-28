target triple = "arm64-apple-macosx11.0.0"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"

; If x is INT64_MAX, sum is poison. A concrete refinement to INT64_MIN must
; not introduce Keel's MIN/-1 trap before the false select masks the result.
define i64 @division_masked(i64 %x, i64 %take) {
entry:
  %sum = add nsw i64 %x, 1
  %quotient = sdiv i64 %sum, -1
  %condition = icmp ne i64 %take, 0
  %result = select i1 %condition, i64 %quotient, i64 7
  ret i64 %result
}
