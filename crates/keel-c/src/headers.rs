//! Compiler-owned headers are embedded so installed binaries need no source tree
//! or Clang resource directory. Native libc comes from the target sysroot;
//! Wasm admits only the explicitly listed freestanding declaration headers.
use std::path::Path;
pub const ROOT: &str = "<keel>/include";
pub fn get_for_target(name: &Path, target: keel_ir::Target) -> Option<&'static str> {
    if target == keel_ir::Target::Wasm32UnknownUnknown
        && !matches!(
            name.to_str()?,
            "stddef.h"
                | "stdint.h"
                | "limits.h"
                | "float.h"
                | "stdbool.h"
                | "stdalign.h"
                | "stdarg.h"
                | "stdnoreturn.h"
        )
    {
        return None;
    }
    get(name)
}
pub fn get(name: &Path) -> Option<&'static str> {
    Some(match name.to_str()? {
        "xmmintrin.h" => include_str!("../include/xmmintrin.h"),
        "wmmintrin.h" => include_str!("../include/wmmintrin.h"),
        "emmintrin.h" => include_str!("../include/emmintrin.h"),
        "pmmintrin.h" => include_str!("../include/pmmintrin.h"),
        "tmmintrin.h" => include_str!("../include/tmmintrin.h"),
        "smmintrin.h" => include_str!("../include/smmintrin.h"),
        "avxintrin.h" => include_str!("../include/avxintrin.h"),
        "avx2intrin.h" => include_str!("../include/avx2intrin.h"),
        "__keel_sse2_conversions.h" => include_str!("../include/__keel_sse2_conversions.h"),
        "__keel_avx512f.h" => include_str!("../include/__keel_avx512f.h"),
        "__keel_avx512_permute.h" => include_str!("../include/__keel_avx512_permute.h"),
        "__keel_avx512_packed.h" => include_str!("../include/__keel_avx512_packed.h"),
        "__keel_avx512_lane.h" => include_str!("../include/__keel_avx512_lane.h"),
        "__keel_avx512_construct.h" => include_str!("../include/__keel_avx512_construct.h"),
        "__keel_avx512_scalar_move.h" => include_str!("../include/__keel_avx512_scalar_move.h"),
        "__keel_avx512_compat.h" => include_str!("../include/__keel_avx512_compat.h"),
        "__keel_avx512_reduce_integer.h" => {
            include_str!("../include/__keel_avx512_reduce_integer.h")
        }
        "__keel_avx512_reduce_float.h" => include_str!("../include/__keel_avx512_reduce_float.h"),
        "__keel_avx512_vl.h" => include_str!("../include/__keel_avx512_vl.h"),
        "__keel_avx512_vl_float.h" => include_str!("../include/__keel_avx512_vl_float.h"),
        "__keel_avx512_vl_integer_move.h" => {
            include_str!("../include/__keel_avx512_vl_integer_move.h")
        }
        "__keel_avx512_vl_convert.h" => include_str!("../include/__keel_avx512_vl_convert.h"),
        "__keel_avx512_cd.h" => include_str!("../include/__keel_avx512_cd.h"),
        "__keel_avx512_dq_bits.h" => include_str!("../include/__keel_avx512_dq_bits.h"),
        "__keel_avx512_bw.h" => include_str!("../include/__keel_avx512_bw.h"),
        "__keel_avx512_vl_fma.h" => include_str!("../include/__keel_avx512_vl_fma.h"),
        "__keel_avx512_dq_convert.h" => include_str!("../include/__keel_avx512_dq_convert.h"),
        "__keel_avx512_bw_mask.h" => include_str!("../include/__keel_avx512_bw_mask.h"),
        "__keel_avx512_dq_mask.h" => include_str!("../include/__keel_avx512_dq_mask.h"),
        "__keel_avx512_vl_special.h" => include_str!("../include/__keel_avx512_vl_special.h"),
        "__keel_avx512_bw_shift.h" => include_str!("../include/__keel_avx512_bw_shift.h"),
        "__keel_avx512_dq_lane.h" => include_str!("../include/__keel_avx512_dq_lane.h"),
        "__keel_avx512_vl_shift.h" => include_str!("../include/__keel_avx512_vl_shift.h"),
        "__keel_avx512_bw_mul.h" => include_str!("../include/__keel_avx512_bw_mul.h"),
        "shaintrin.h" => include_str!("../include/shaintrin.h"),
        "__keel_aes.h" => include_str!("../include/__keel_aes.h"),
        "fmaintrin.h" => include_str!("../include/fmaintrin.h"),
        "f16cintrin.h" => include_str!("../include/f16cintrin.h"),
        "immintrin.h" => include_str!("../include/immintrin.h"),
        "__keel_sse2_lane_ops.h" => include_str!("../include/__keel_sse2_lane_ops.h"),
        "__keel_sse2_construct.h" => include_str!("../include/__keel_sse2_construct.h"),
        "__keel_sse3_duplicate.h" => include_str!("../include/__keel_sse3_duplicate.h"),
        "__keel_ssse3_lane_ops.h" => include_str!("../include/__keel_ssse3_lane_ops.h"),
        "__keel_sse41_widen.h" => include_str!("../include/__keel_sse41_widen.h"),
        "__keel_sse41_lane_ops.h" => include_str!("../include/__keel_sse41_lane_ops.h"),
        "__keel_avx_lane_ops.h" => include_str!("../include/__keel_avx_lane_ops.h"),
        "__keel_avx_construct.h" => include_str!("../include/__keel_avx_construct.h"),
        "__keel_avx2_widen.h" => include_str!("../include/__keel_avx2_widen.h"),
        "__keel_avx2_lane_ops.h" => include_str!("../include/__keel_avx2_lane_ops.h"),
        "__keel_avx2_broadcast.h" => include_str!("../include/__keel_avx2_broadcast.h"),
        "__keel_sse_float.h" => include_str!("../include/__keel_sse_float.h"),
        "__keel_sse2_float.h" => include_str!("../include/__keel_sse2_float.h"),
        "__keel_avx_float.h" => include_str!("../include/__keel_avx_float.h"),
        "__keel_sse_construct.h" => include_str!("../include/__keel_sse_construct.h"),
        "__keel_sse_conversions.h" => include_str!("../include/__keel_sse_conversions.h"),
        "__keel_sse2_numeric.h" => include_str!("../include/__keel_sse2_numeric.h"),
        "__keel_avx_conversions.h" => include_str!("../include/__keel_avx_conversions.h"),
        "__keel_sse41_queries.h" => include_str!("../include/__keel_sse41_queries.h"),
        "__keel_avx_queries.h" => include_str!("../include/__keel_avx_queries.h"),
        "__keel_avx2_special.h" => include_str!("../include/__keel_avx2_special.h"),
        "__keel_sse_memory.h" => include_str!("../include/__keel_sse_memory.h"),
        "__keel_sse2_memory.h" => include_str!("../include/__keel_sse2_memory.h"),
        "__keel_sse3_memory.h" => include_str!("../include/__keel_sse3_memory.h"),
        "__keel_sse_compare.h" => include_str!("../include/__keel_sse_compare.h"),
        "__keel_sse2_compare.h" => include_str!("../include/__keel_sse2_compare.h"),
        "__keel_avx_compare.h" => include_str!("../include/__keel_avx_compare.h"),
        "__keel_avx_mask_memory.h" => include_str!("../include/__keel_avx_mask_memory.h"),
        "__keel_avx2_mask_memory.h" => include_str!("../include/__keel_avx2_mask_memory.h"),
        "__keel_sse_state.h" => include_str!("../include/__keel_sse_state.h"),
        "__keel_sse2_state.h" => include_str!("../include/__keel_sse2_state.h"),
        "__keel_sse41_state.h" => include_str!("../include/__keel_sse41_state.h"),
        "__keel_avx_state.h" => include_str!("../include/__keel_avx_state.h"),
        "__keel_avx2_state.h" => include_str!("../include/__keel_avx2_state.h"),
        "__keel_sse42_strings.h" => include_str!("../include/__keel_sse42_strings.h"),
        "crc32intrin.h" => include_str!("../include/crc32intrin.h"),
        "nmmintrin.h" => include_str!("../include/nmmintrin.h"),
        "__keel_sse_convenience.h" => include_str!("../include/__keel_sse_convenience.h"),
        "__keel_sse2_convenience.h" => include_str!("../include/__keel_sse2_convenience.h"),
        "__keel_sse3_convenience.h" => include_str!("../include/__keel_sse3_convenience.h"),
        "__keel_sse41_convenience.h" => include_str!("../include/__keel_sse41_convenience.h"),
        "__keel_avx_convenience.h" => include_str!("../include/__keel_avx_convenience.h"),
        "stddef.h" => include_str!("../include/stddef.h"),
        "stdarg.h" => include_str!("../include/stdarg.h"),
        "stdint.h" => include_str!("../include/stdint.h"),
        "limits.h" => include_str!("../include/limits.h"),
        "float.h" => include_str!("../include/float.h"),
        "stdbool.h" => include_str!("../include/stdbool.h"),
        "stdalign.h" => include_str!("../include/stdalign.h"),
        "stdnoreturn.h" => include_str!("../include/stdnoreturn.h"),
        "stdatomic.h" => include_str!("../include/stdatomic.h"),
        "ptrcheck.h" => include_str!("../include/ptrcheck.h"),
        _ => return None,
    })
}
