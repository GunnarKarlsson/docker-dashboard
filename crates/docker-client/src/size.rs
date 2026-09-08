/// Parse a Docker CLI size such as `92.81GB`, `2.53kB`, or `0B`.
///
/// Docker prints decimal SI units. A trailing ` (100%)` on reclaimable
/// fields is ignored. Unknown or empty input becomes `0`.
pub fn parse_docker_size(text: &str) -> u64 {
    let token = text
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('%');

    if token.is_empty() || token == "0" || token == "0B" {
        return 0;
    }

    let split = token
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(token.len());
    let (number, unit) = token.split_at(split);
    let value: f64 = number.parse().unwrap_or(0.0);
    let multiplier = match unit {
        "" | "B" => 1.0,
        "kB" | "KB" | "K" => 1_000.0,
        "MB" | "M" => 1_000_000.0,
        "GB" | "G" => 1_000_000_000.0,
        "TB" | "T" => 1_000_000_000_000.0,
        "PB" | "P" => 1_000_000_000_000_000.0,
        _ => 1.0,
    };

    (value * multiplier).round() as u64
}

#[cfg(test)]
mod tests {
    use super::parse_docker_size;

    #[test]
    fn parses_si_units_and_percent_suffix() {
        assert_eq!(parse_docker_size("0B"), 0);
        assert_eq!(parse_docker_size("2.53kB"), 2530);
        assert_eq!(parse_docker_size("8.825MB"), 8_825_000);
        assert_eq!(parse_docker_size("92.81GB"), 92_810_000_000);
        assert_eq!(parse_docker_size("92.81GB (100%)"), 92_810_000_000);
        assert_eq!(parse_docker_size("0B (0%)"), 0);
        assert_eq!(parse_docker_size("15.08GB"), 15_080_000_000);
    }
}
