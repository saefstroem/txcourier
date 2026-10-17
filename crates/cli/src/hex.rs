use std::str::from_utf8;

use txcourier_protocol::transaction::ADDRESS_LEN;

pub fn of(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn address(bytes: &[u8]) -> String {
    prefixed(bytes)
}

pub fn prefixed(bytes: &[u8]) -> String {
    let digits: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("0x{digits}")
}

pub fn parse_address(text: &str) -> Option<[u8; ADDRESS_LEN]> {
    parse(text)?.try_into().ok()
}

pub fn parse(text: &str) -> Option<Vec<u8>> {
    let packed: String = text
        .chars()
        .filter(|glyph| !glyph.is_whitespace())
        .collect();
    let digits = packed.strip_prefix("0x").unwrap_or(&packed).as_bytes();

    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return None;
    }

    digits
        .chunks(2)
        .map(|pair| u8::from_str_radix(from_utf8(pair).ok()?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::address;
    use super::of;
    use super::parse;
    use super::parse_address;
    use super::prefixed;

    const RECIPIENT: [u8; 20] = [
        0xd8, 0xda, 0x6b, 0xf2, 0x69, 0x64, 0xaf, 0x9d, 0x7e, 0xed, 0x9e, 0x03, 0xe5, 0x34, 0x15,
        0xd3, 0x7a, 0xa9, 0x60, 0x45,
    ];

    #[test]
    fn an_address_prints_as_one_prefixed_lowercase_word() {
        assert_eq!(
            address(&RECIPIENT),
            "0xd8da6bf26964af9d7eed9e03e53415d37aa96045"
        );
    }

    #[test]
    fn an_address_parses_with_or_without_the_prefix_and_in_any_case() {
        for text in [
            "0xd8da6bf26964af9d7eed9e03e53415d37aa96045",
            "d8da6bf26964af9d7eed9e03e53415d37aa96045",
            "0xD8dA6BF26964aF9D7eEd9e03E53415D37aA96045",
        ] {
            assert_eq!(parse_address(text), Some(RECIPIENT), "{text}");
        }
    }

    #[test]
    fn an_address_of_the_wrong_length_is_refused() {
        assert_eq!(
            parse_address("0xd8da6bf26964af9d7eed9e03e53415d37aa960"),
            None
        );
        assert_eq!(
            parse_address("0xd8da6bf26964af9d7eed9e03e53415d37aa9604500"),
            None
        );
        assert_eq!(parse_address("0x"), None);
    }

    #[test]
    fn prefixed_bytes_are_one_lowercase_word() {
        assert_eq!(prefixed(&[0x02, 0xF8, 0x00]), "0x02f800");
        assert_eq!(prefixed(&[]), "0x");
    }

    #[test]
    fn bytes_are_lowercase_and_space_separated() {
        assert_eq!(of(&[0x80, 0xD2, 0x00]), "80 d2 00");
    }

    #[test]
    fn no_bytes_is_an_empty_string() {
        assert_eq!(of(&[]), "");
    }

    #[test]
    fn hex_round_trips_with_and_without_the_prefix() {
        assert_eq!(parse("80d200").as_deref(), Some(&[0x80, 0xD2, 0x00][..]));
        assert_eq!(parse("0x80d200").as_deref(), Some(&[0x80, 0xD2, 0x00][..]));
        assert_eq!(
            parse(&of(&[0x02, 0xF0, 0x01])).as_deref(),
            Some(&[0x02, 0xF0, 0x01][..])
        );
    }

    #[test]
    fn the_spaced_form_that_the_slot_command_prints_can_be_pushed_back() {
        assert_eq!(parse(" 80 d2 \n").as_deref(), Some(&[0x80, 0xD2][..]));
    }

    #[test]
    fn uppercase_is_accepted() {
        assert_eq!(parse("80D2").as_deref(), Some(&[0x80, 0xD2][..]));
    }

    #[test]
    fn an_odd_or_empty_or_dirty_string_is_refused() {
        assert_eq!(parse("80d"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("0x"), None);
        assert_eq!(parse("80zz"), None);
    }
}
