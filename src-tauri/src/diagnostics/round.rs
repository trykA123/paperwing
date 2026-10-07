pub fn count(value: u64) -> u64 {
    if value < 100 {
        return value;
    }
    let scale = 10_u64.pow(value.ilog10() - 1);
    let rounded = value / scale + u64::from(value % scale >= scale / 2);
    rounded.saturating_mul(scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_sizes_and_counts_to_two_significant_figures() {
        assert_eq!(count(1534), 1500);
        assert_eq!(count(42_700_000), 43_000_000);
        assert_eq!(count(42), 42);
        assert_eq!(count(0), 0);
    }

    #[test]
    fn rounds_large_memory_counts_exactly() {
        assert_eq!(count(32_999_999_999), 33_000_000_000);
        assert_eq!(count(33_000_000_000), 33_000_000_000);
    }

    #[test]
    fn preserves_small_counts_and_rounds_the_u64_limit_without_overflow() {
        for value in [0, 1, 99] {
            assert_eq!(count(value), value);
        }
        assert_eq!(count(u64::MAX), 18_000_000_000_000_000_000);
        assert_eq!(
            count(18_400_000_000_000_000_001),
            18_000_000_000_000_000_000
        );
    }

    #[test]
    fn rounds_halfway_up_and_carries_to_the_next_magnitude() {
        assert_eq!(count(1549), 1500);
        assert_eq!(count(1550), 1600);
        assert_eq!(count(999), 1000);
        assert_eq!(count(9_949), 9_900);
        assert_eq!(count(9_950), 10_000);
    }
}
