target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
define i64 @bool_sext(i64 %x) {
entry:
  %b = trunc i64 %x to i1
  %v = sext i1 %b to i64
  ret i64 %v
}
define i64 @pointer_bit(ptr %p) {
entry:
  %b = ptrtoint ptr %p to i1
  %v = zext i1 %b to i64
  ret i64 %v
}
define zeroext i8 @narrow_return(i64 %x) {
entry:
  %v = trunc i64 %x to i8
  ret i8 %v
}
