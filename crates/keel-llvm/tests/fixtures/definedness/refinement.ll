target triple = "arm64-apple-macosx11.0.0"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"

define i64 @masked(i1 zeroext %take) {
  %r = select i1 %take, i64 poison, i64 7
  ret i64 %r
}

define i64 @masked_phi(i1 zeroext %take) {
  br i1 %take, label %bad, label %good
bad:
  br label %join
good:
  br label %join
join:
  %r = phi i64 [poison, %bad], [9, %good]
  ret i64 %r
}

define i64 @identity(i64 %x) {
  %r = freeze i64 %x
  ret i64 %r
}

define double @fp_identity(double %x) {
  %r = freeze double %x
  ret double %r
}

define ptr @ptr_identity(ptr %x) {
  %r = freeze ptr %x
  ret ptr %r
}

define i64 @stable_poison() {
  %x = freeze i64 poison
  %a = add i64 %x, 1
  %b = add i64 %x, 1
  %r = xor i64 %a, %b
  ret i64 %r
}

define i64 @stable_undef() {
  %x = add i64 undef, 5
  %f = freeze i64 %x
  %r = xor i64 %f, %f
  ret i64 %r
}

define i64 @undef_range() {
  %x = add i64 undef, undef
  %f = freeze i64 %x
  %r = and i64 %f, 255
  ret i64 %r
}

define i64 @flagged(i64 %x) {
  %p = add nsw i64 %x, 1
  %r = freeze i64 %p
  ret i64 %r
}

define i64 @flagged_stable() {
  %p = add nsw i64 9223372036854775807, 1
  %f = freeze i64 %p
  %r = xor i64 %f, %f
  ret i64 %r
}

define i64 @loop_freeze(i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i64 [0, %entry], [%next, %loop]
  %sum = phi i64 [0, %entry], [%acc, %loop]
  %f = freeze i64 undef
  %same = xor i64 %f, %f
  %acc = add i64 %sum, %same
  %next = add i64 %i, 1
  %more = icmp ult i64 %next, %n
  br i1 %more, label %loop, label %exit
exit:
  ret i64 %acc
}
