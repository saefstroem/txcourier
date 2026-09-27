use std::str::from_utf8;

pub fn of(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
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
    use super::of;
    use super::parse;

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
