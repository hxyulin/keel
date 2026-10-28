/// Representative legal immediates: endpoints, their neighbors and the midpoint.
/// Opcode/shape matrices retain every operation; interior counts share lowering.
pub fn boundaries(range: std::ops::RangeInclusive<i64>) -> Vec<i64> {
    assert!(!range.is_empty());
    let (start, end) = (*range.start(), *range.end());
    let mut values = vec![start, start + 1, start + (end - start) / 2, end - 1, end];
    values.retain(|value| range.contains(value));
    values.sort_unstable();
    values.dedup();
    values
}
