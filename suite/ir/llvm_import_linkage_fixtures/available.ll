target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
define available_externally i64 @available_helper(i64 %x) {
  %y = xor i64 %x, 23
  ret i64 %y
}
define i64 @available_entry(i64 %x) {
  %y = call i64 @available_helper(i64 %x)
  ret i64 %y
}
; No dead-definition pruning: keep the unused ODR body and its local callee.
define internal i64 @private_helper(i64 %x) {
  %y = add i64 %x, 7
  ret i64 %y
}
define linkonce_odr i64 @unused_odr_helper(i64 %x) {
  %y = call i64 @private_helper(i64 %x)
  ret i64 %y
}
