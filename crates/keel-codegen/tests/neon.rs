//! NEON intrinsic records: one module per record class, each checked against native or clang oracles.
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
#[allow(dead_code)]
#[path = "common/immediates.rs"]
mod immediates;
#[allow(dead_code)]
#[path = "common/matrix.rs"]
mod matrix;
#[path = "neon/neon_architectural.rs"]
mod neon_architectural;
#[path = "neon/neon_conversions.rs"]
mod neon_conversions;
#[path = "neon/neon_immediate_shifts.rs"]
mod neon_immediate_shifts;
#[path = "neon/neon_indexed.rs"]
mod neon_indexed;
#[path = "neon/neon_integer_families.rs"]
mod neon_integer_families;
#[path = "neon/neon_integer_misc.rs"]
mod neon_integer_misc;
#[path = "neon/neon_inventory.rs"]
mod neon_inventory;
#[path = "neon/neon_lane_matrix.rs"]
mod neon_lane_matrix;
#[path = "neon/neon_lane_records.rs"]
mod neon_lane_records;
#[path = "neon/neon_long_narrow.rs"]
mod neon_long_narrow;
#[path = "neon/neon_modified_immediates.rs"]
mod neon_modified_immediates;
#[path = "neon/neon_portable_records.rs"]
mod neon_portable_records;
#[path = "neon/neon_postindex.rs"]
mod neon_postindex;
#[path = "neon/neon_recipes.rs"]
mod neon_recipes;
#[path = "neon/neon_reductions.rs"]
mod neon_reductions;
#[path = "neon/neon_scalar.rs"]
mod neon_scalar;
#[path = "neon/neon_single_lane.rs"]
mod neon_single_lane;
#[path = "neon/neon_structured.rs"]
mod neon_structured;
#[path = "neon/neon_tables.rs"]
mod neon_tables;
#[path = "neon/neon_width_changes.rs"]
mod neon_width_changes;
#[path = "neon/neon_width_halves.rs"]
mod neon_width_halves;
