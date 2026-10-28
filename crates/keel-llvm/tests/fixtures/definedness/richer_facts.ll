target triple = "arm64-apple-macosx11.0.0"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"

declare noalias ptr @fresh(i64)

define noundef range(i64 0, 256) i64 @bounded(i64 noundef range(i64 0, 128) %x) {
  %r = add i64 %x, 1
  ret i64 %r
}

define i64 @indirect_bounded(ptr %fn, i64 %x) {
  %r = call noundef range(i64 0, 256) i64 %fn(i64 noundef range(i64 0, 128) %x)
  ret i64 %r
}

define range(i64 1, 8) i64 @poison_range(i64 %x) {
  ret i64 %x
}

define i64 @masked_range(i64 %x) {
  %p = call range(i64 1, 8) i64 @poison_range(i64 %x)
  %r = select i1 false, i64 %p, i64 73
  ret i64 %r
}

define ptr @returned_copy(ptr captures(ret: address, provenance) %p) {
  ret ptr %p
}

define i64 @nullable(ptr noundef dereferenceable_or_null(8) %p) {
  %null = icmp eq ptr %p, null
  br i1 %null, label %empty, label %read
empty:
  ret i64 0
read:
  %r = load i64, ptr %p, align 1
  ret i64 %r
}

define i64 @allocation(i64 %value) {
  %p = call noalias ptr @fresh(i64 8)
  store i64 %value, ptr %p, align 1
  %r = load i64, ptr %p, align 1
  ret i64 %r
}
