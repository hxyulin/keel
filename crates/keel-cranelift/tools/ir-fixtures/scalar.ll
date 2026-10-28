target datalayout = "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
define i64 @probe(i64 %a, i64 %b) {
entry:
  %cond = icmp ult i64 %a, %b
  br i1 %cond, label %yes, label %no
yes:
  %sum = add i64 %a, %b
  %amount = and i64 %b, 63
  %shift = shl i64 %a, %amount
  %mixed = xor i64 %sum, %shift
  br label %done
no:
  %diff = sub i64 %a, %b
  br label %done
done:
  %result = phi i64 [ %mixed, %yes ], [ %diff, %no ]
  ret i64 %result
}
