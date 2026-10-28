target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
define weak i64 @replaceable(i64 %x) { ret i64 %x }
define i64 @interposition_entry(i64 %x) {
  %y = call i64 @replaceable(i64 %x)
  ret i64 %y
}
