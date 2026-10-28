target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
@fp_negative_zero = global double -0.000000e+00
@fp_nan = constant double 0x7FF0000000000001
@fp_subnormal = constant float 0x36A0000000000000
@fp_nested = global { i8, double, [2 x float] } { i8 7, double -0.000000e+00, [2 x float] [float 0x7FF0000020000000, float 1.500000e+00] }
@fp_zero = global { double, [2 x float] } zeroinitializer
@fp_external = external global double
@fp_field = constant ptr getelementptr ({ i8, double, [2 x float] }, ptr @fp_nested, i64 0, i32 2, i64 1)
@fp_callback = global ptr @fp_external_callback
declare double @fp_external_callback(double)
define double @fp_apply_global(double %x) {
 %fn = load ptr, ptr @fp_callback
 %r = call double %fn(double %x)
 %external = load double, ptr @fp_external
 %sum = fadd double %r, %external
 ret double %sum
}
define ptr @fp_field_address() {
 ret ptr getelementptr ({ i8, double, [2 x float] }, ptr @fp_nested, i64 0, i32 2, i64 1)
}
