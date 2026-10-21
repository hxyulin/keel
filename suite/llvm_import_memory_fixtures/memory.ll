target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
declare void @llvm.memcpy.p0.p0.i64(ptr noalias nocapture writeonly, ptr noalias nocapture readonly, i64, i1 immarg)
declare void @llvm.memmove.p0.p0.i64(ptr nocapture writeonly, ptr nocapture readonly, i64, i1 immarg)
declare void @llvm.memset.p0.i64(ptr nocapture writeonly, i8, i64, i1 immarg)
define void @copy(ptr %d, ptr %s, i64 %n) {
 call void @llvm.memcpy.p0.p0.i64(ptr align 1 %d, ptr align 1 %s, i64 %n, i1 false)
 ret void
}
define void @move(ptr %d, ptr %s, i64 %n) {
 call void @llvm.memmove.p0.p0.i64(ptr %d, ptr %s, i64 %n, i1 false)
 ret void
}
define void @fill(ptr %d, i8 zeroext %b, i64 %n) {
 call void @llvm.memset.p0.i64(ptr %d, i8 %b, i64 %n, i1 false)
 ret void
}
define void @copy31(ptr %d, ptr %s) {
 call void @llvm.memcpy.p0.p0.i64(ptr %d, ptr %s, i64 31, i1 false)
 ret void
}
