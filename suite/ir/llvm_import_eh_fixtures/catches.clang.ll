; ModuleID = 'suite/llvm_import_eh_fixtures/catches.cpp'
source_filename = "suite/llvm_import_eh_fixtures/catches.cpp"
target datalayout = "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx27.0.0"

@_ZTIi = external constant ptr

; Function Attrs: mustprogress ssp uwtable(sync)
define i32 @eh_catches(i32 noundef %0) local_unnamed_addr #0 personality ptr @__gxx_personality_v0 {
  %2 = invoke i32 @eh_throw(i32 noundef %0)
          to label %3 unwind label %5

3:                                                ; preds = %1
  %4 = add i32 %2, 3
  br label %16

5:                                                ; preds = %1
  %6 = landingpad { ptr, i32 }
          catch ptr @_ZTIi
          catch ptr null
  %7 = extractvalue { ptr, i32 } %6, 0
  %8 = extractvalue { ptr, i32 } %6, 1
  %9 = tail call i32 @llvm.eh.typeid.for.p0(ptr nonnull @_ZTIi) #4
  %10 = icmp eq i32 %8, %9
  %11 = tail call ptr @__cxa_begin_catch(ptr %7) #4
  br i1 %10, label %12, label %15

12:                                               ; preds = %5
  %13 = load i32, ptr %11, align 4, !tbaa !6
  %14 = add i32 %13, 101
  tail call void @__cxa_end_catch() #4
  br label %16

15:                                               ; preds = %5
  tail call void @__cxa_end_catch()
  br label %16

16:                                               ; preds = %3, %15, %12
  %17 = phi i32 [ %4, %3 ], [ %14, %12 ], [ -303, %15 ]
  ret i32 %17
}

declare i32 @eh_throw(i32 noundef) local_unnamed_addr #1

declare i32 @__gxx_personality_v0(...)

; Function Attrs: nofree nosync nounwind memory(none)
declare i32 @llvm.eh.typeid.for.p0(ptr) #2

declare ptr @__cxa_begin_catch(ptr) local_unnamed_addr

declare void @__cxa_end_catch() local_unnamed_addr

; Function Attrs: mustprogress ssp uwtable(sync)
define i32 @eh_typed(i32 noundef %0) local_unnamed_addr #0 personality ptr @__gxx_personality_v0 {
  %2 = invoke i32 @eh_throw(i32 noundef %0)
          to label %3 unwind label %5

3:                                                ; preds = %1
  %4 = add i32 %2, 5
  br label %15

5:                                                ; preds = %1
  %6 = landingpad { ptr, i32 }
          catch ptr @_ZTIi
  %7 = extractvalue { ptr, i32 } %6, 1
  %8 = tail call i32 @llvm.eh.typeid.for.p0(ptr nonnull @_ZTIi) #4
  %9 = icmp eq i32 %7, %8
  br i1 %9, label %10, label %17

10:                                               ; preds = %5
  %11 = extractvalue { ptr, i32 } %6, 0
  %12 = tail call ptr @__cxa_begin_catch(ptr %11) #4
  %13 = load i32, ptr %12, align 4, !tbaa !6
  %14 = add i32 %13, 201
  tail call void @__cxa_end_catch() #4
  br label %15

15:                                               ; preds = %3, %10
  %16 = phi i32 [ %4, %3 ], [ %14, %10 ]
  ret i32 %16

17:                                               ; preds = %5
  resume { ptr, i32 } %6
}

; Function Attrs: mustprogress ssp uwtable(sync)
define i32 @eh_cleaned(i32 noundef %0) local_unnamed_addr #0 personality ptr @__gxx_personality_v0 {
  %2 = invoke i32 @eh_throw(i32 noundef %0)
          to label %3 unwind label %5

3:                                                ; preds = %1
  %4 = add i32 %2, 7
  tail call void @eh_cleanup(i32 noundef %0) #4
  ret i32 %4

5:                                                ; preds = %1
  %6 = landingpad { ptr, i32 }
          cleanup
  tail call void @eh_cleanup(i32 noundef %0) #4
  resume { ptr, i32 } %6
}

; Function Attrs: mustprogress ssp uwtable(sync)
define i32 @eh_rethrow(i32 noundef %0) local_unnamed_addr #0 personality ptr @__gxx_personality_v0 {
  %2 = invoke i32 @eh_throw(i32 noundef %0)
          to label %3 unwind label %5

3:                                                ; preds = %1
  %4 = add i32 %2, 9
  ret i32 %4

5:                                                ; preds = %1
  %6 = landingpad { ptr, i32 }
          catch ptr @_ZTIi
  %7 = extractvalue { ptr, i32 } %6, 1
  %8 = tail call i32 @llvm.eh.typeid.for.p0(ptr nonnull @_ZTIi) #4
  %9 = icmp eq i32 %7, %8
  br i1 %9, label %10, label %15

10:                                               ; preds = %5
  %11 = extractvalue { ptr, i32 } %6, 0
  %12 = tail call ptr @__cxa_begin_catch(ptr %11) #4
  tail call void @eh_cleanup(i32 noundef %0) #4
  invoke void @__cxa_rethrow() #5
          to label %17 unwind label %13

13:                                               ; preds = %10
  %14 = landingpad { ptr, i32 }
          cleanup
  tail call void @__cxa_end_catch() #4
  br label %15

15:                                               ; preds = %13, %5
  %16 = phi { ptr, i32 } [ %14, %13 ], [ %6, %5 ]
  resume { ptr, i32 } %16

17:                                               ; preds = %10
  unreachable
}

; Function Attrs: nounwind
declare void @eh_cleanup(i32 noundef) local_unnamed_addr #3

declare void @__cxa_rethrow() local_unnamed_addr

attributes #0 = { mustprogress ssp uwtable(sync) "frame-pointer"="non-leaf" "no-trapping-math"="true" "probe-stack"="__chkstk_darwin" "stack-protector-buffer-size"="8" "target-cpu"="apple-m1" "target-features"="+aes,+altnzcv,+bti,+ccdp,+ccidx,+ccpp,+complxnum,+crc,+dit,+dotprod,+flagm,+fp-armv8,+fp16fml,+fptoint,+fullfp16,+jsconv,+lse,+neon,+pauth,+perfmon,+predres,+ras,+rcpc,+rdm,+sb,+sha2,+sha3,+specrestrict,+ssbs,+v8.1a,+v8.2a,+v8.3a,+v8.4a,+v8.5a,+v8a" }
attributes #1 = { "frame-pointer"="non-leaf" "no-trapping-math"="true" "probe-stack"="__chkstk_darwin" "stack-protector-buffer-size"="8" "target-cpu"="apple-m1" "target-features"="+aes,+altnzcv,+bti,+ccdp,+ccidx,+ccpp,+complxnum,+crc,+dit,+dotprod,+flagm,+fp-armv8,+fp16fml,+fptoint,+fullfp16,+jsconv,+lse,+neon,+pauth,+perfmon,+predres,+ras,+rcpc,+rdm,+sb,+sha2,+sha3,+specrestrict,+ssbs,+v8.1a,+v8.2a,+v8.3a,+v8.4a,+v8.5a,+v8a" }
attributes #2 = { nofree nosync nounwind memory(none) }
attributes #3 = { nounwind "frame-pointer"="non-leaf" "no-trapping-math"="true" "probe-stack"="__chkstk_darwin" "stack-protector-buffer-size"="8" "target-cpu"="apple-m1" "target-features"="+aes,+altnzcv,+bti,+ccdp,+ccidx,+ccpp,+complxnum,+crc,+dit,+dotprod,+flagm,+fp-armv8,+fp16fml,+fptoint,+fullfp16,+jsconv,+lse,+neon,+pauth,+perfmon,+predres,+ras,+rcpc,+rdm,+sb,+sha2,+sha3,+specrestrict,+ssbs,+v8.1a,+v8.2a,+v8.3a,+v8.4a,+v8.5a,+v8a" }
attributes #4 = { nounwind }
attributes #5 = { noreturn }

!llvm.module.flags = !{!0, !1, !2, !3, !4}
!llvm.ident = !{!5}

!0 = !{i32 2, !"SDK Version", [2 x i32] [i32 27, i32 0]}
!1 = !{i32 1, !"wchar_size", i32 4}
!2 = !{i32 8, !"PIC Level", i32 2}
!3 = !{i32 7, !"uwtable", i32 1}
!4 = !{i32 7, !"frame-pointer", i32 1}
!5 = !{!"Apple clang version 21.0.0 (clang-2100.3.34.2)"}
!6 = !{!7, !7, i64 0}
!7 = !{!"int", !8, i64 0}
!8 = !{!"omnipotent char", !9, i64 0}
!9 = !{!"Simple C++ TBAA"}
