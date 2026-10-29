//! x86-64 scalar instruction selection: memory operands, addressing, compares, branches and division.
#[path = "x86_scalar_selection/x86_brif_fallthrough.rs"]
mod x86_brif_fallthrough;
#[path = "x86_scalar_selection/x86_c_leaf.rs"]
mod x86_c_leaf;
#[path = "x86_scalar_selection/x86_compare_select.rs"]
mod x86_compare_select;
#[path = "x86_scalar_selection/x86_constant_division.rs"]
mod x86_constant_division;
#[path = "x86_scalar_selection/x86_indexed_memory.rs"]
mod x86_indexed_memory;
#[path = "x86_scalar_selection/x86_lea.rs"]
mod x86_lea;
#[path = "x86_scalar_selection/x86_mask_test.rs"]
mod x86_mask_test;
#[path = "x86_scalar_selection/x86_memory_cmp.rs"]
mod x86_memory_cmp;
#[path = "x86_scalar_selection/x86_memory_rhs.rs"]
mod x86_memory_rhs;
#[path = "x86_scalar_selection/x86_signed_byte_load.rs"]
mod x86_signed_byte_load;
#[path = "x86_scalar_selection/x86_size.rs"]
mod x86_size;
