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
        "smmintrin.h" => include_str!("../include/smmintrin.h"),
        "avxintrin.h" => include_str!("../include/avxintrin.h"),
        "avx2intrin.h" => include_str!("../include/avx2intrin.h"),
        "__keel_sse2_conversions.h" => include_str!("../include/__keel_sse2_conversions.h"),
        "__keel_aes.h" => include_str!("../include/__keel_aes.h"),
        "immintrin.h" => include_str!("../include/immintrin.h"),
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
