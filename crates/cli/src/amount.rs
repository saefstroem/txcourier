pub const NATIVE_DECIMALS: u32 = 18;
pub const GWEI_DECIMALS: u32 = 9;
pub const MAX_DECIMALS: u32 = 38;

pub fn parse(text: &str, decimals: u32) -> Option<u128> {
    let text = text.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));

    if whole.is_empty() && fraction.is_empty() {
        return None;
    }

    let digits_only = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if !digits_only(whole) || !digits_only(fraction) {
        return None;
    }

    let width = usize::try_from(decimals).ok()?;
    if fraction.len() > width {
        return None;
    }

    let scale = 10u128.checked_pow(decimals)?;
    let whole: u128 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };

    let padded = format!("{fraction:0<width$}");
    let fraction: u128 = if padded.is_empty() {
        0
    } else {
        padded.parse().ok()?
    };

    whole.checked_mul(scale)?.checked_add(fraction)
}

pub fn format(value: u128, decimals: u32) -> String {
    let (Some(scale), Ok(width)) = (10u128.checked_pow(decimals), usize::try_from(decimals)) else {
        return format!("{value}e-{decimals}");
    };

    let whole = value / scale;
    let fraction = value % scale;

    if fraction == 0 {
        return whole.to_string();
    }

    let digits = format!("{fraction:0>width$}");
    format!("{whole}.{}", digits.trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::{MAX_DECIMALS, format, parse};

    const ETHER: u32 = 18;
    const GWEI: u32 = 9;
    const USDT: u32 = 6;

    #[test]
    fn a_decimal_amount_becomes_whole_base_units() {
        assert_eq!(parse("1", ETHER), Some(1_000_000_000_000_000_000));
        assert_eq!(parse("12.5", USDT), Some(12_500_000));
        assert_eq!(parse("0.000001", USDT), Some(1));
        assert_eq!(parse("1.5", GWEI), Some(1_500_000_000));
    }

    #[test]
    fn a_missing_whole_or_fraction_part_is_zero() {
        assert_eq!(parse(".5", USDT), Some(500_000));
        assert_eq!(parse("7.", USDT), Some(7_000_000));
        assert_eq!(parse("  3  ", USDT), Some(3_000_000));
    }

    #[test]
    fn more_decimal_places_than_the_unit_has_are_refused() {
        assert_eq!(parse("0.0000001", USDT), None);
        assert_eq!(parse("1.0000000000000000001", ETHER), None);
    }

    #[test]
    fn anything_but_digits_and_one_point_is_refused() {
        for text in ["", ".", "-1", "+1", "1e3", "1,5", "1.2.3", "0x10", "one"] {
            assert_eq!(parse(text, ETHER), None, "{text:?} parsed");
        }
    }

    #[test]
    fn an_amount_too_large_to_hold_is_refused() {
        assert_eq!(parse("340282366920938463464", ETHER), None);
        assert_eq!(parse("1", 39), None);
    }

    #[test]
    fn base_units_print_as_the_shortest_decimal() {
        assert_eq!(format(1_000_000_000_000_000_000, ETHER), "1");
        assert_eq!(format(12_500_000, USDT), "12.5");
        assert_eq!(format(1, USDT), "0.000001");
        assert_eq!(format(0, USDT), "0");
        assert_eq!(format(42, 0), "42");
    }

    #[test]
    fn the_most_decimals_a_u128_can_scale_by_is_the_limit() {
        assert_eq!(10u128.checked_pow(MAX_DECIMALS), Some(10u128.pow(38)));
        assert_eq!(10u128.checked_pow(MAX_DECIMALS + 1), None);
        assert_eq!(parse("1", MAX_DECIMALS), Some(10u128.pow(38)));
        assert_eq!(parse("1", MAX_DECIMALS + 1), None);
        assert_eq!(format(1, MAX_DECIMALS), format!("0.{}1", "0".repeat(37)));
    }

    #[test]
    fn an_amount_with_more_decimals_than_the_limit_prints_in_scientific_form() {
        assert_eq!(format(42, 39), "42e-39");
        assert_eq!(format(0, 4_000_000_000), "0e-4000000000");
    }

    #[test]
    fn printing_and_parsing_agree() {
        for value in [0, 1, 7, 12_500_000, 999_999_999, 1_000_000_000_000_000_001] {
            assert_eq!(parse(&format(value, ETHER), ETHER), Some(value));
            assert_eq!(parse(&format(value, USDT), USDT), Some(value));
        }
    }
}
