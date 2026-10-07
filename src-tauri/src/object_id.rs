pub(crate) fn valid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_full_sha1_and_sha256_and_rejects_abbreviations_or_nonhex() {
        for length in [40, 64] {
            assert!(valid(&"a".repeat(length)));
            assert!(valid(&"F".repeat(length)));
            assert!(!valid(&format!("{}g", "a".repeat(length - 1))));
        }
        for length in [0, 7, 39, 41, 63, 65] {
            assert!(!valid(&"a".repeat(length)));
        }
        assert!(!valid(&"é".repeat(20)));
    }
}
