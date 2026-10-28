target triple = "arm64-apple-macosx11.0.0"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
define i64 @read_only(ptr noalias %a, ptr noalias %b) {
  %x = load i64, ptr %a
  %y = load i64, ptr %b
  %result = add i64 %x, %y
  ret i64 %result
}
define i64 @write_distinct(ptr noalias %a, ptr noalias %b) {
  %x = load i64, ptr %a
  store i64 9, ptr %b
  %y = load i64, ptr %a
  %result = add i64 %x, %y
  ret i64 %result
}
define i64 @pointer_equality(ptr noalias %a, ptr noalias %b) {
  %eq = icmp eq ptr %a, %b
  %result = zext i1 %eq to i64
  ret i64 %result
}
define i64 @masked(ptr nonnull %p, i64 %take, i64 %a, i64 %b) {
  %condition = icmp ne i64 %take, 0
  %chosen = select i1 %condition, ptr %p, ptr null
  %isnull = icmp eq ptr %chosen, null
  %sum = add nsw i64 %a, %b
  %result = select i1 %isnull, i64 7, i64 %sum
  ret i64 %result
}
