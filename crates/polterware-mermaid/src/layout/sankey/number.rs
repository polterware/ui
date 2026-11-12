//! JavaScript's `String(number)`, which d3-path 1.0.9 uses when it concatenates the
//! coordinates of a path (`this._ += "M" + (+x) + "," + (+y)`).
//!
//! Rust's shortest round-trip digits and JavaScript's are the same digits; the
//! difference is layout: JavaScript writes exponent notation below `1e-6` and from
//! `1e21`, and trims a `.0`. This formats from the shortest significant digits
//! (`{:e}`) with ECMAScript's `Number::toString` rules, so a path string matches the
//! one d3-sankey draws.

/// Formats `value` as `String(value)` would in JavaScript.
pub(crate) fn format_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if value == 0.0 {
        // Covers -0, which JavaScript prints as "0".
        return "0".to_owned();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    // "1.234e2": the shortest digits that round-trip, and the exponent of the first.
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').expect("an exponent");
    let exponent: i32 = exponent.parse().expect("an integer exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    // The decimal point sits k digits into the significand, and `n` digits in from
    // the start; ECMAScript keys every case off `n` and `k`.
    let n = exponent + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{}", "0".repeat((-n) as usize), digits)
    } else {
        let exponent = n - 1;
        let mantissa = if k == 1 {
            digits
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!(
            "{mantissa}e{}{}",
            if exponent >= 0 { "+" } else { "-" },
            exponent.abs()
        )
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::format_number;

    /// The expected strings are what Node prints for these numbers.
    #[test]
    fn matches_javascript() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-1.0, "-1"),
            (24.0, "24"),
            (0.5, "0.5"),
            (100.0, "100"),
            (123.456, "123.456"),
            (0.000001, "0.000001"),
            (0.0000001, "1e-7"),
            (1e-21, "1e-21"),
            (1e21, "1e+21"),
            (1e22, "1e+22"),
            (123456789012345680000.0, "123456789012345680000"),
            (1.2345678901234567, "1.2345678901234567"),
            (-2.5e-7, "-2.5e-7"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
        ];
        for &(value, expected) in cases {
            assert_eq!(format_number(value), expected, "{value:?}");
        }
    }
}
