use regex::Regex;

const SEPARATOR: char = '_';

#[derive(Clone, Copy)]
enum Base {
    Base2 = 2,
    Base8 = 8,
    Base10 = 10,
    Base16 = 16,
}

struct NumberPattern {
    base: Base,
    prefix: String,
}

/// Increment an integer.
///
/// Supported bases:
///     2 with prefix 0b
///     8 with prefix 0o
///     10 with no prefix
///     16 with prefix 0x
///
/// An integer can contain `_` as a separator but may not start or end with a separator.
/// Base 10 integers can go negative, but bases 2, 8, and 16 cannot.
/// All addition and subtraction is saturating.
pub fn increment(selected_text: &str, amount: i64) -> Option<String> {
    if selected_text.is_empty()
        || selected_text.ends_with(SEPARATOR)
        || selected_text.starts_with(SEPARATOR)
    {
        return None;
    }

    let patterns = vec![
        (Base::Base16, Regex::new(r"^(0x|\d*'h)").unwrap()),
        (Base::Base10, Regex::new(r"^(\d*'d)").unwrap()),
        (Base::Base8, Regex::new(r"^(0o)").unwrap()),
        (Base::Base2, Regex::new(r"^(0b|\d*'b)").unwrap()),
    ];

    let pattern = patterns
        .iter()
        .find_map(|(base, matcher)| match matcher.find(selected_text) {
            Some(m) => Some(NumberPattern {
                base: base.to_owned(),
                prefix: m.as_str().to_string(),
            }),
            None => None,
        });

    let pattern = match pattern {
        Some(p) => p,
        None => NumberPattern {
            base: Base::Base10,
            prefix: "".to_owned(),
        },
    };

    log::warn!(
        "base = {:?}; prefix {:?} ({})",
        pattern.base as u32,
        pattern.prefix,
        pattern.prefix.len()
    );

    // Get separator indexes from right to left.
    let separator_rtl_indexes: Vec<usize> = selected_text
        .chars()
        .rev()
        .enumerate()
        .filter_map(|(i, c)| if c == SEPARATOR { Some(i) } else { None })
        .collect();

    let word: String = selected_text.chars().filter(|&c| c != SEPARATOR).collect();

    let mut new_text = match pattern.base {
        Base::Base10 => {
            let number = &word;
            let value = i128::from_str_radix(number, pattern.base as u32).ok()?;
            let new_value = value.saturating_add(amount as i128);

            let format_length = match (value.is_negative(), new_value.is_negative()) {
                (true, false) => number.len() - 1,
                (false, true) => number.len() + 1,
                _ => number.len(),
            } - separator_rtl_indexes.len();

            if number.starts_with('0') || number.starts_with("-0") {
                format!("{:01$}", new_value, format_length)
            } else {
                format!("{}", new_value)
            }
        }
        _ => {
            let number = &word[pattern.prefix.len()..];
            log::warn!("Number = {:?}", number);
            let value = u128::from_str_radix(number, pattern.base as u32).ok()?;
            let new_value = (value as i128).saturating_add(amount as i128);
            let new_value = if new_value < 0 { 0 } else { new_value };
            let format_length =
                selected_text.len() - pattern.prefix.len() - separator_rtl_indexes.len();

            match pattern.base {
                Base::Base2 => format!("{2}{:01$b}", new_value, format_length, pattern.prefix),
                Base::Base8 => format!("{2}{:01$o}", new_value, format_length, pattern.prefix),
                Base::Base16 => {
                    let (lower_count, upper_count): (usize, usize) =
                        number.chars().fold((0, 0), |(lower, upper), c| {
                            (
                                lower + c.is_ascii_lowercase() as usize,
                                upper + c.is_ascii_uppercase() as usize,
                            )
                        });
                    if upper_count > lower_count {
                        format!("{2}{:01$X}", new_value, format_length, pattern.prefix)
                    } else {
                        format!("{2}{:01$x}", new_value, format_length, pattern.prefix)
                    }
                }
                _ => unimplemented!("radix not supported: {}", pattern.base as u32),
            }
        }
    };

    // Add separators from original number.
    for &rtl_index in &separator_rtl_indexes {
        if rtl_index < new_text.len() {
            let new_index = new_text.len().saturating_sub(rtl_index);
            if new_index > 0 {
                new_text.insert(new_index, SEPARATOR);
            }
        }
    }

    // Add in additional separators if necessary.
    if new_text.len() > selected_text.len() && !separator_rtl_indexes.is_empty() {
        let spacing = match separator_rtl_indexes.as_slice() {
            [.., b, a] => a - b - 1,
            _ => separator_rtl_indexes[0],
        };

        let prefix_length = pattern.prefix.len();
        if let Some(mut index) = new_text.find(SEPARATOR) {
            while index - prefix_length > spacing {
                index -= spacing;
                new_text.insert(index, SEPARATOR);
            }
        }
    }

    Some(new_text)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_increment_basic_decimal_numbers() {
        let tests = [
            ("100", 1, "101"),
            ("100", -1, "99"),
            ("99", 1, "100"),
            ("100", 1000, "1100"),
            ("100", -1000, "-900"),
            ("-1", 1, "0"),
            ("-1", 2, "1"),
            ("1", -1, "0"),
            ("1", -2, "-1"),
        ];

        for (original, amount, expected) in tests {
            assert_eq!(increment(original, amount).unwrap(), expected);
        }
    }

    #[test]
    fn test_increment_basic_hexadecimal_numbers() {
        let tests = [
            ("0x0100", 1, "0x0101"),
            ("0x0100", -1, "0x00ff"),
            ("0x0001", -1, "0x0000"),
            ("0x0000", -1, "0x0000"),
            ("16'h0100", 1, "16'h0101"),
            ("16'h0100", -1, "16'h00ff"),
            ("16'h0001", -1, "16'h0000"),
            ("16'h0000", -1, "16'h0000"),
            ("'h0100", 1, "'h0101"),
            ("'h0100", -1, "'h00ff"),
            ("'h0001", -1, "'h0000"),
            ("'h0000", -1, "'h0000"),
            ("0xffffffffffffffff", 1, "0x10000000000000000"),
            ("0xffffffffffffffff", 2, "0x10000000000000001"),
            ("0xffffffffffffffff", -1, "0xfffffffffffffffe"),
            ("0xABCDEF1234567890", 1, "0xABCDEF1234567891"),
            ("0xabcdef1234567890", 1, "0xabcdef1234567891"),
            ("64'hffffffffffffffff", 1, "64'h10000000000000000"),
            ("64'hffffffffffffffff", 2, "64'h10000000000000001"),
            ("64'hffffffffffffffff", -1, "64'hfffffffffffffffe"),
            ("64'hABCDEF1234567890", 1, "64'hABCDEF1234567891"),
            ("64'habcdef1234567890", 1, "64'habcdef1234567891"),
        ];

        for (original, amount, expected) in tests {
            assert_eq!(increment(original, amount).unwrap(), expected);
        }
    }

    #[test]
    fn test_increment_basic_octal_numbers() {
        let tests = [
            ("0o0107", 1, "0o0110"),
            ("0o0110", -1, "0o0107"),
            ("0o0001", -1, "0o0000"),
            ("0o7777", 1, "0o10000"),
            ("0o1000", -1, "0o0777"),
            ("0o0107", 10, "0o0121"),
            ("0o0000", -1, "0o0000"),
            ("0o1777777777777777777777", 1, "0o2000000000000000000000"),
            ("0o1777777777777777777777", 2, "0o2000000000000000000001"),
            ("0o1777777777777777777777", -1, "0o1777777777777777777776"),
        ];

        for (original, amount, expected) in tests {
            assert_eq!(increment(original, amount).unwrap(), expected);
        }
    }

    #[test]
    fn test_increment_basic_binary_numbers() {
        let tests = [
            ("0b00000100", 1, "0b00000101"),
            ("0b00000100", -1, "0b00000011"),
            ("0b00000100", 2, "0b00000110"),
            ("0b00000100", -2, "0b00000010"),
            ("0b00000001", -1, "0b00000000"),
            ("0b00111111", 10, "0b01001001"),
            ("0b11111111", 1, "0b100000000"),
            ("0b10000000", -1, "0b01111111"),
            ("0b0000", -1, "0b0000"),
            (
                "0b1111111111111111111111111111111111111111111111111111111111111111",
                1,
                "0b10000000000000000000000000000000000000000000000000000000000000000",
            ),
            (
                "0b1111111111111111111111111111111111111111111111111111111111111111",
                2,
                "0b10000000000000000000000000000000000000000000000000000000000000001",
            ),
            (
                "0b1111111111111111111111111111111111111111111111111111111111111111",
                -1,
                "0b1111111111111111111111111111111111111111111111111111111111111110",
            ),
            ("128'b00000100", 1, "128'b00000101"),
            ("128'b00000100", -1, "128'b00000011"),
            ("128'b00000100", 2, "128'b00000110"),
            ("128'b00000100", -2, "128'b00000010"),
            ("128'b00000001", -1, "128'b00000000"),
            ("128'b00111111", 10, "128'b01001001"),
            ("128'b11111111", 1, "128'b100000000"),
            ("128'b10000000", -1, "128'b01111111"),
            ("128'b0000", -1, "128'b0000"),
            (
                "128'b1111111111111111111111111111111111111111111111111111111111111111",
                1,
                "128'b10000000000000000000000000000000000000000000000000000000000000000",
            ),
            (
                "128'b1111111111111111111111111111111111111111111111111111111111111111",
                2,
                "128'b10000000000000000000000000000000000000000000000000000000000000001",
            ),
            (
                "128'b1111111111111111111111111111111111111111111111111111111111111111",
                -1,
                "128'b1111111111111111111111111111111111111111111111111111111111111110",
            ),
        ];

        for (original, amount, expected) in tests {
            assert_eq!(increment(original, amount).unwrap(), expected);
        }
    }

    #[test]
    fn test_increment_with_separators() {
        let tests = [
            ("999_999", 1, "1_000_000"),
            ("1_000_000", -1, "999_999"),
            ("-999_999", -1, "-1_000_000"),
            ("0x0000_0000_0001", 0x1_ffff_0000, "0x0001_ffff_0001"),
            ("0x0000_0000", -1, "0x0000_0000"),
            ("0x0000_0000_0000", -1, "0x0000_0000_0000"),
            ("64'h0000_0000_0001", 0x1_ffff_0000, "64'h0001_ffff_0001"),
            ("64'h0000_0000", -1, "64'h0000_0000"),
            ("64'h0000_0000_0000", -1, "64'h0000_0000_0000"),
            ("0b01111111_11111111", 1, "0b10000000_00000000"),
            ("0b11111111_11111111", 1, "0b1_00000000_00000000"),
        ];

        for (original, amount, expected) in tests {
            assert_eq!(increment(original, amount).unwrap(), expected);
        }
    }

    #[test]
    fn test_leading_and_trailing_separators_arent_a_match() {
        assert_eq!(increment("9_", 1), None);
        assert_eq!(increment("_9", 1), None);
        assert_eq!(increment("_9_", 1), None);
    }
}
