; Purpose-built CFG fixture, alongside unmodified compiler-emitted catches.ll.
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx14.0.0"
declare i32 @__gxx_personality_v0(...)
declare i32 @eh_throw(i32)
declare ptr @__cxa_begin_catch(ptr)
declare void @__cxa_end_catch()
define i32 @eh_edges(i32 %x, i32 %side) personality ptr @__gxx_personality_v0 {
entry:
  %cond = icmp ne i32 %side, 0
  br i1 %cond, label %a, label %b
a:
  %sa = xor i32 %x, 23
  %ra = invoke i32 @eh_throw(i32 %x) to label %normal unwind label %pad
b:
  %sb = xor i32 %x, 11
  %rb = invoke i32 @eh_throw(i32 %x) to label %normal unwind label %pad
normal:
  %r = phi i32 [ %ra, %a ], [ %rb, %b ]
  %base = phi i32 [ 23, %a ], [ 11, %b ]
  %sum = add i32 %r, %base
  ret i32 %sum
pad:
  %saved = phi i32 [ %sa, %a ], [ %sb, %b ]
  %lp = landingpad {ptr, i32} catch ptr null
  %ex = extractvalue {ptr, i32} %lp, 0
  %obj = call ptr @__cxa_begin_catch(ptr %ex)
  call void @__cxa_end_catch()
  %caught = add i32 %saved, 100
  ret i32 %caught
}
