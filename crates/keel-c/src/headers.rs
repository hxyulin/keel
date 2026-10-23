//! Compiler-owned headers are embedded so installed binaries need no source tree
//! or Clang resource directory. Libc remains supplied by the target sysroot.
use std::path::Path;
pub const ROOT: &str = "<keel>/include";
pub fn get(name: &Path) -> Option<&'static str> {
    Some(match name.to_str()? {
        "xmmintrin.h" => include_str!("../include/xmmintrin.h"),
        "wmmintrin.h" => include_str!("../include/wmmintrin.h"),
        "emmintrin.h" => include_str!("../include/emmintrin.h"),
        "smmintrin.h" => include_str!("../include/smmintrin.h"),
        "avxintrin.h" => include_str!("../include/avxintrin.h"),
        "avx2intrin.h" => include_str!("../include/avx2intrin.h"),
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
