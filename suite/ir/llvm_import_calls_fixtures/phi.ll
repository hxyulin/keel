target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
declare i64 @external_add(i64, i64)
declare i64 @local_xor(i64, i64)
define i64 @phi_call(ptr %f, i32 %choose, i64 %x, i64 %y) {
entry:
 %test = icmp eq i32 %choose, 0
 br i1 %test, label %a, label %b
a:
 br label %join
b:
 br label %join
join:
 %target = phi ptr [ %f, %a ], [ @local_xor, %b ]
 %r = call i64 %target(i64 %x, i64 %y)
 ret i64 %r
}
define i64 @address_stack(i64 %x, i64 %y) {
entry:
 %p = alloca ptr, align 8
 store ptr @external_add, ptr %p
 %target = load ptr, ptr %p
 %r = call i64 %target(i64 %x, i64 %y)
 ret i64 %r
}
