target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
declare i64 @callback(i64)
define internal fastcc i64 @mixed(i64 %a0, i64 %a1, i64 %a2, i64 %a3, i64 %a4, i64 %a5, i64 %a6, i64 %a7, i64 %a8, i64 %a9, i64 %a10, i64 %a11, double %f0, double %f1, double %f2, double %f3, double %f4, double %f5, double %f6, double %f7, double %f8, double %f9) noinline {
entry:
 %s1 = add i64 %a0, %a1
 %s2 = add i64 %s1, %a2
 %s3 = add i64 %s2, %a3
 %s4 = add i64 %s3, %a4
 %s5 = add i64 %s4, %a5
 %s6 = add i64 %s5, %a6
 %s7 = add i64 %s6, %a7
 %s8 = add i64 %s7, %a8
 %s9 = add i64 %s8, %a9
 %s10 = add i64 %s9, %a10
 %s11 = add i64 %s10, %a11
 %j0 = fptoui double %f0 to i64
 %t0 = add i64 %s11, %j0
 %j1 = fptoui double %f1 to i64
 %t1 = add i64 %t0, %j1
 %j2 = fptoui double %f2 to i64
 %t2 = add i64 %t1, %j2
 %j3 = fptoui double %f3 to i64
 %t3 = add i64 %t2, %j3
 %j4 = fptoui double %f4 to i64
 %t4 = add i64 %t3, %j4
 %j5 = fptoui double %f5 to i64
 %t5 = add i64 %t4, %j5
 %j6 = fptoui double %f6 to i64
 %t6 = add i64 %t5, %j6
 %j7 = fptoui double %f7 to i64
 %t7 = add i64 %t6, %j7
 %j8 = fptoui double %f8 to i64
 %t8 = add i64 %t7, %j8
 %j9 = fptoui double %f9 to i64
 %t9 = add i64 %t8, %j9
 %r = call i64 @callback(i64 %t9)
 %out = add i64 %r, %t9
 ret i64 %out
}
define private fastcc i64 @recursive(i64 %n) noinline {
entry:
 %test = icmp eq i64 %n, 0
 br i1 %test, label %done, label %again
done:
 ret i64 0
again:
 %less = sub i64 %n, 1
 %r = call fastcc i64 @recursive(i64 %less)
 %out = add i64 %r, %n
 ret i64 %out
}
define private fastcc i8 @narrow(i8 %x) noinline {
entry:
 %r = add i8 %x, -5
 ret i8 %r
}
define private fastcc i1 @predicate(i64 %x) noinline {
entry:
 %r = icmp ne i64 %x, 0
 ret i1 %r
}
define i64 @entry(i64 %seed) {
entry:
 %a0 = add i64 %seed, 0
 %a1 = add i64 %seed, 1
 %a2 = add i64 %seed, 2
 %a3 = add i64 %seed, 3
 %a4 = add i64 %seed, 4
 %a5 = add i64 %seed, 5
 %a6 = add i64 %seed, 6
 %a7 = add i64 %seed, 7
 %a8 = add i64 %seed, 8
 %a9 = add i64 %seed, 9
 %a10 = add i64 %seed, 10
 %a11 = add i64 %seed, 11
 %mixed = call fastcc i64 @mixed(i64 %a0, i64 %a1, i64 %a2, i64 %a3, i64 %a4, i64 %a5, i64 %a6, i64 %a7, i64 %a8, i64 %a9, i64 %a10, i64 %a11, double 0.000000e+00, double 1.000000e+00, double 2.000000e+00, double 3.000000e+00, double 4.000000e+00, double 5.000000e+00, double 6.000000e+00, double 7.000000e+00, double 8.000000e+00, double 9.000000e+00)
 %n = and i64 %seed, 7
 %recursive = call fastcc i64 @recursive(i64 %n)
 %byte = trunc i64 %seed to i8
 %narrow = call fastcc i8 @narrow(i8 %byte)
 %wide = zext i8 %narrow to i64
 %s = add i64 %mixed, %recursive
 %out = add i64 %s, %wide
 %flag = call fastcc i1 @predicate(i64 %seed)
 %boolean = zext i1 %flag to i64
 %out2 = add i64 %out, %boolean
 ret i64 %out2
}
