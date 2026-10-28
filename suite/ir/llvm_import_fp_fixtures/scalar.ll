target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"


define double @fadd64(double %a, double %b) { %v = fadd double %a, %b
ret double %v }

define double @fsub64(double %a, double %b) { %v = fsub double %a, %b
ret double %v }

define double @fmul64(double %a, double %b) { %v = fmul double %a, %b
ret double %v }

define double @fdiv64(double %a, double %b) { %v = fdiv double %a, %b
ret double %v }

define double @neg64(double %a) { %v = fneg double %a
ret double %v }

define i32 @cmp_false64(double %a, double %b) { %v = fcmp false double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_oeq64(double %a, double %b) { %v = fcmp oeq double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ogt64(double %a, double %b) { %v = fcmp ogt double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_oge64(double %a, double %b) { %v = fcmp oge double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_olt64(double %a, double %b) { %v = fcmp olt double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ole64(double %a, double %b) { %v = fcmp ole double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_one64(double %a, double %b) { %v = fcmp one double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ord64(double %a, double %b) { %v = fcmp ord double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_uno64(double %a, double %b) { %v = fcmp uno double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ueq64(double %a, double %b) { %v = fcmp ueq double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ugt64(double %a, double %b) { %v = fcmp ugt double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_uge64(double %a, double %b) { %v = fcmp uge double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ult64(double %a, double %b) { %v = fcmp ult double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ule64(double %a, double %b) { %v = fcmp ule double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_une64(double %a, double %b) { %v = fcmp une double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_true64(double %a, double %b) { %v = fcmp true double %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @fptosi64_i32(double %a) { %v = fptosi double %a to i32
ret i32 %v }

define i64 @fptosi64_i64(double %a) { %v = fptosi double %a to i64
ret i64 %v }

define i32 @fptoui64_i32(double %a) { %v = fptoui double %a to i32
ret i32 %v }

define i64 @fptoui64_i64(double %a) { %v = fptoui double %a to i64
ret i64 %v }

define double @sitofp64_i8(i8 %a) { %v = sitofp i8 %a to double
ret double %v }

define double @sitofp64_i16(i16 %a) { %v = sitofp i16 %a to double
ret double %v }

define double @sitofp64_i32(i32 %a) { %v = sitofp i32 %a to double
ret double %v }

define double @sitofp64_i64(i64 %a) { %v = sitofp i64 %a to double
ret double %v }

define double @uitofp64_i8(i8 %a) { %v = uitofp i8 %a to double
ret double %v }

define double @uitofp64_i16(i16 %a) { %v = uitofp i16 %a to double
ret double %v }

define double @uitofp64_i32(i32 %a) { %v = uitofp i32 %a to double
ret double %v }

define double @uitofp64_i64(i64 %a) { %v = uitofp i64 %a to double
ret double %v }

define i64 @bits64(double %a) { %v = bitcast double %a to i64
ret i64 %v }

define double @frombits64(i64 %a) { %v = bitcast i64 %a to double
ret double %v }

define float @fadd32(float %a, float %b) { %v = fadd float %a, %b
ret float %v }

define float @fsub32(float %a, float %b) { %v = fsub float %a, %b
ret float %v }

define float @fmul32(float %a, float %b) { %v = fmul float %a, %b
ret float %v }

define float @fdiv32(float %a, float %b) { %v = fdiv float %a, %b
ret float %v }

define float @neg32(float %a) { %v = fneg float %a
ret float %v }

define i32 @cmp_false32(float %a, float %b) { %v = fcmp false float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_oeq32(float %a, float %b) { %v = fcmp oeq float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ogt32(float %a, float %b) { %v = fcmp ogt float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_oge32(float %a, float %b) { %v = fcmp oge float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_olt32(float %a, float %b) { %v = fcmp olt float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ole32(float %a, float %b) { %v = fcmp ole float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_one32(float %a, float %b) { %v = fcmp one float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ord32(float %a, float %b) { %v = fcmp ord float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_uno32(float %a, float %b) { %v = fcmp uno float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ueq32(float %a, float %b) { %v = fcmp ueq float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ugt32(float %a, float %b) { %v = fcmp ugt float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_uge32(float %a, float %b) { %v = fcmp uge float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ult32(float %a, float %b) { %v = fcmp ult float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_ule32(float %a, float %b) { %v = fcmp ule float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_une32(float %a, float %b) { %v = fcmp une float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @cmp_true32(float %a, float %b) { %v = fcmp true float %a, %b
%z = zext i1 %v to i32
ret i32 %z }

define i32 @fptosi32_i32(float %a) { %v = fptosi float %a to i32
ret i32 %v }

define i64 @fptosi32_i64(float %a) { %v = fptosi float %a to i64
ret i64 %v }

define i32 @fptoui32_i32(float %a) { %v = fptoui float %a to i32
ret i32 %v }

define i64 @fptoui32_i64(float %a) { %v = fptoui float %a to i64
ret i64 %v }

define float @sitofp32_i8(i8 %a) { %v = sitofp i8 %a to float
ret float %v }

define float @sitofp32_i16(i16 %a) { %v = sitofp i16 %a to float
ret float %v }

define float @sitofp32_i32(i32 %a) { %v = sitofp i32 %a to float
ret float %v }

define float @sitofp32_i64(i64 %a) { %v = sitofp i64 %a to float
ret float %v }

define float @uitofp32_i8(i8 %a) { %v = uitofp i8 %a to float
ret float %v }

define float @uitofp32_i16(i16 %a) { %v = uitofp i16 %a to float
ret float %v }

define float @uitofp32_i32(i32 %a) { %v = uitofp i32 %a to float
ret float %v }

define float @uitofp32_i64(i64 %a) { %v = uitofp i64 %a to float
ret float %v }

define i32 @bits32(float %a) { %v = bitcast float %a to i32
ret i32 %v }

define float @frombits32(i32 %a) { %v = bitcast i32 %a to float
ret float %v }

define float @narrow(double %a) { %v = fptrunc double %a to float
ret float %v }

define double @widen(float %a) { %v = fpext float %a to double
ret double %v }

define double @memory(double %a, float %b) {
 %p = alloca {float, double}, align 8
 %q = getelementptr {float, double}, ptr %p, i64 0, i32 1
 store double %a, ptr %q
 %v = load double, ptr %q
 %w = call double @widen(float %b)
 %r = fadd double %v, %w
 ret double %r
}

define double @zero64() { ret double 0x8000000000000000 }

define double @snan64() { ret double 0x7FF0000000000001 }

define double @tiny64() { ret double 0x0000000000000001 }

define float @snan32() { ret float 0x7FF0000020000000 }

define float @tiny32() { ret float 0x36A0000000000000 }

define float @tie() { %v = fadd float 16777216.0, 1.0
ret float %v }

define double @signedzero() { %v = fmul double -0.0, 2.0
ret double %v }

; i1 true is -1 for sitofp, +1 for uitofp. No i1 ABI is exposed.
define double @boolsigned(i32 %a) {
 %c = icmp ne i32 %a, 0
 %v = sitofp i1 %c to double
 ret double %v
}
define float @boolunsigned(i32 %a) {
 %c = icmp ne i32 %a, 0
 %v = uitofp i1 %c to float
 ret float %v
}
define double @choose(i32 %a, double %x, double %y) {
 %c = icmp ne i32 %a, 0
 br i1 %c, label %yes, label %no
yes:
 br label %exit
no:
 br label %exit
exit:
 %r = phi double [ %x, %yes ], [ %y, %no ]
 ret double %r
}
define float @memory32(float %a, float %b, i32 %c) {
 %p = alloca [3 x float], align 4
 %q = getelementptr [3 x float], ptr %p, i64 0, i64 2
 store float %a, ptr %q
 %v = load float, ptr %q
 %cond = icmp ne i32 %c, 0
 %r = select i1 %cond, float %v, float %b
 ret float %r
}
; Enough FP parameters to use stack arguments; mixed integer/FP registers.
define double @many(double %a, double %b, double %c, double %d, double %e, double %f, double %g, double %h, double %i, double %j, i64 %n) {
 %x = fadd double %a, %j
 %y = fadd double %x, %i
 %z = sitofp i64 %n to double
 %r = fadd double %y, %z
 ret double %r
}
define double @manycall(double %a) {
 %r = call double @many(double %a, double 2.0, double 3.0, double 4.0, double 5.0, double 6.0, double 7.0, double 8.0, double 9.0, double 10.0, i64 -3)
 ret double %r
}
; Foldable conversions: all FP->integer inputs are LLVM-defined.
define i64 @foldsi() {
 %r = fptosi double -9223372036854775808.0 to i64
 ret i64 %r
}

define i64 @foldui() {
 %r = fptoui double 18446744073709549568.0 to i64
 ret i64 %r
}
define float @foldfrom() {
 %r = uitofp i64 -1 to float
 ret float %r
}
define float @foldnarrow() {
 %r = fptrunc double 1.000000059604644775390625 to float
 ret float %r
}
define double @foldwiden() {
 %r = fpext float 0x36A0000000000000 to double
 ret double %r
}
define i32 @foldnan() {
 %r = fcmp one double 0x7FF8000000000001, 1.0
 %z = zext i1 %r to i32
 ret i32 %z
}
define i32 @folduno() {
 %r = fcmp ueq float 0x7FF8000000000000, 0.0
 %z = zext i1 %r to i32
 ret i32 %z
}
define signext i8 @smallret(i8 signext %a) { ret i8 %a }
define signext i8 @smallcall(i8 signext %a) {
 %v = call signext i8 @smallret(i8 signext %a)
 ret i8 %v
}

declare double @external_fp(double, float)
define double @outside(double %a) {
 %r = call double @external_fp(double %a, float 0.5)
 ret double %r
}
define double @foldneg() {
 %v = fneg double 0x7FF0000000000001
 ret double %v
}
define i32 @foldbits() {
 %v = bitcast float 0x7FF0000020000000 to i32
 ret i32 %v
}

define double @indirect_fp(ptr %fn, double %a, float %b) {
 %r = call double %fn(double %a, float %b)
 ret double %r
}
define float @indirect_fp32(ptr %fn, float %a) {
 %r = call float %fn(float %a)
 ret float %r
}
