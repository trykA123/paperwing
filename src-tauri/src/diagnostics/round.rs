pub fn significant(value: f64, digits: u32) -> f64 {
    if value == 0.0 || !value.is_finite() || digits == 0 {
        return value;
    }
    let magnitude = value.abs().log10().floor();
    let factor = 10_f64.powf(f64::from(digits - 1) - magnitude);
    (value * factor).round() / factor
}

pub fn count(value: u64) -> u64 {
    significant(value as f64, 2).max(0.0) as u64
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
    fn preserves_non_finite_and_zero_measurements() {
        assert_eq!(significant(0.0, 2), 0.0);
        assert!(significant(f64::INFINITY, 2).is_infinite());
        assert!(significant(f64::NAN, 2).is_nan());
    }
}
