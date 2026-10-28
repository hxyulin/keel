target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
%R = type { i8, [2 x { i16, i64 }], ptr }
@nested = global %R { i8 7, [2 x { i16, i64 }] [{ i16, i64 } { i16 -11, i64 13 }, { i16, i64 } { i16 17, i64 -19 }], ptr @nested }, align 16
@zero_record = global %R zeroinitializer
@bits = constant [3 x i1] [i1 true, i1 false, i1 true]
@one_past = constant ptr getelementptr (%R, ptr @nested, i64 1)
@nested_field = constant ptr getelementptr (%R, ptr @nested, i64 0, i32 1, i64 1, i32 1)
@external_array = external global [4 x i32]
@external_element = global ptr getelementptr ([4 x i32], ptr @external_array, i64 0, i64 3)
@private_integer = private global i64 -77
@private_pointer = global ptr @private_integer
@external_const = external constant i32
define ptr @constant_expression() {
  ret ptr getelementptr (%R, ptr @nested, i64 0, i32 1, i64 1, i32 1)
}
define i32 @read_external_const() {
  %v = load i32, ptr @external_const
  ret i32 %v
}
@packed_aligned = global <{ ptr, i8 }> <{ ptr @nested, i8 21 }>, align 8
@packed_null = global <{ i8, ptr }> <{ i8 22, ptr null }>, align 1
@pointer_array = global [3 x ptr] [ptr @nested, ptr null, ptr getelementptr (%R, ptr @nested, i64 1)]
@max_addend = global ptr getelementptr (i8, ptr @external_array, i64 4294967295)
define ptr @max_addend_address() {
  ret ptr getelementptr (i8, ptr @external_array, i64 4294967295)
}
