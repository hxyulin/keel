//! Feature detection for the native compiler environment. Unknown capabilities
//! answer zero; parsing an ignored attribute does not establish support.
use keel_ir::Target;

pub fn query(name: &str, argument: &str, target: Target) -> u64 {
    let arg = argument
        .strip_prefix("__")
        .and_then(|s| s.strip_suffix("__"))
        .unwrap_or(argument);
    let apple = target == Target::Aarch64AppleDarwin;
    let yes = match name {
        "__has_builtin" => builtin(argument, target),
        "__has_feature" | "__has_extension" => matches!(
            arg,
            "c_alignas"
                | "c_alignof"
                | "c_atomic"
                | "c_generic_selections"
                | "c_static_assert"
                | "c_thread_local"
        ),
        "__has_attribute" => matches!(arg, "aligned" | "packed" | "returns_twice"),
        "__has_c_attribute" | "__has_declspec_attribute" | "__building_module" => false,
        "__is_target_arch" => {
            if apple {
                matches!(arg, "aarch64" | "arm64")
            } else {
                arg == "x86_64"
            }
        }
        "__is_target_vendor" => arg == if apple { "apple" } else { "unknown" },
        "__is_target_os" => arg == if apple { "macos" } else { "linux" },
        "__is_target_environment" => arg == if apple { "unknown" } else { "gnu" },
        "__is_target_variant_os" | "__is_target_variant_environment" => false,
        "__is_identifier" => !matches!(
            argument,
            "auto"
                | "break"
                | "case"
                | "char"
                | "const"
                | "continue"
                | "default"
                | "do"
                | "double"
                | "else"
                | "enum"
                | "extern"
                | "float"
                | "for"
                | "goto"
                | "if"
                | "inline"
                | "int"
                | "long"
                | "register"
                | "restrict"
                | "return"
                | "short"
                | "signed"
                | "sizeof"
                | "static"
                | "struct"
                | "switch"
                | "typedef"
                | "union"
                | "unsigned"
                | "void"
                | "volatile"
                | "while"
                | "_Alignas"
                | "_Alignof"
                | "_Atomic"
                | "_Bool"
                | "_Complex"
                | "_Generic"
                | "_Imaginary"
                | "_Noreturn"
                | "_Static_assert"
                | "_Thread_local"
                | "__attribute__"
                | "__typeof__"
                | "__typeof"
                | "typeof"
                | "__auto_type"
                | "__int128"
        ),
        _ => false,
    };
    u64::from(yes)
}

fn builtin(name: &str, target: Target) -> bool {
    if matches!(
        name,
        "__is_target_arch" | "__is_target_vendor" | "__is_target_os" | "__is_target_environment"
    ) {
        return true;
    }
    let Some(base) = name.strip_prefix("__builtin_") else {
        return name.strip_prefix("__c11_atomic_").is_some_and(|op| {
            matches!(
                op,
                "init"
                    | "load"
                    | "store"
                    | "exchange"
                    | "compare_exchange_strong"
                    | "compare_exchange_weak"
                    | "fetch_add"
                    | "fetch_sub"
                    | "fetch_and"
                    | "fetch_or"
                    | "fetch_xor"
                    | "thread_fence"
                    | "signal_fence"
            )
        });
    };
    if crate::expr::typed_overflow(base).is_some() {
        return true;
    }
    if target == Target::X86_64LinuxGnu
        && matches!(base, "infl" | "huge_vall" | "nanl" | "fabsl" | "signbitl")
    {
        return false;
    }
    matches!(
        base,
        "va_start"
            | "va_end"
            | "va_arg"
            | "va_copy"
            | "offsetof"
            | "expect"
            | "expect_with_probability"
            | "assume_aligned"
            | "object_size"
            | "dynamic_object_size"
            | "types_compatible_p"
            | "choose_expr"
            | "constant_p"
            | "inf"
            | "inff"
            | "infl"
            | "huge_val"
            | "huge_valf"
            | "huge_vall"
            | "nan"
            | "nanf"
            | "nanl"
            | "isnan"
            | "isinf"
            | "isinf_sign"
            | "isfinite"
            | "isnormal"
            | "signbit"
            | "signbitf"
            | "signbitl"
            | "fpclassify"
            | "isgreater"
            | "isgreaterequal"
            | "isless"
            | "islessequal"
            | "islessgreater"
            | "isunordered"
            | "fabs"
            | "fabsf"
            | "fabsl"
            | "bswap16"
            | "bswap32"
            | "bswap64"
            | "clz"
            | "ctz"
            | "popcount"
            | "clzl"
            | "ctzl"
            | "popcountl"
            | "clzll"
            | "ctzll"
            | "popcountll"
            | "alloca"
            | "frame_address"
            | "trap"
            | "unreachable"
            | "debugtrap"
    )
}
