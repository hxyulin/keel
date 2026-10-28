target triple = "arm64-apple-macosx11.0.0"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"

declare void @overwrite_cell(ptr)

define i64 @local_range(ptr %cell) {
  %v = load i64, ptr %cell, align 8, !range !0, !noundef !1
  %r = add i64 %v, 1
  ret i64 %r
}

define i64 @masked_local_range(ptr %cell) {
  %p = load i64, ptr %cell, align 8, !range !2
  %r = select i1 false, i64 %p, i64 73
  ret i64 %r
}

define i64 @local_pointer(ptr %cell) {
  %p = load ptr, ptr %cell, align 8, !nonnull !1, !align !3, !dereferenceable !4, !noundef !1
  %v = load i64, ptr %p, align 1
  ret i64 %v
}

define i64 @masked_local_pointer(ptr %cell) {
  %p = load ptr, ptr %cell, align 8, !nonnull !1, !align !3
  %q = select i1 false, ptr %p, ptr null
  %null = icmp eq ptr %q, null
  %r = zext i1 %null to i64
  ret i64 %r
}

define i64 @local_nullable(ptr %cell) {
  %p = load ptr, ptr %cell, align 8, !dereferenceable_or_null !4, !noundef !1
  %null = icmp eq ptr %p, null
  br i1 %null, label %empty, label %read
empty:
  ret i64 0
read:
  %v = load i64, ptr %p, align 1
  ret i64 %v
}

define i64 @separate_loads(ptr %cell) {
  %first = load i64, ptr %cell, align 8, !range !0, !noundef !1
  call void @overwrite_cell(ptr %cell)
  %second = load i64, ptr %cell, align 8
  %r = add i64 %first, %second
  ret i64 %r
}

!0 = !{i64 0, i64 128}
!1 = !{}
!2 = !{i64 1, i64 8}
!3 = !{i64 16}
!4 = !{i64 32}
