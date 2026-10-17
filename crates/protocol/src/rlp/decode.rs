use crate::rlp::error::Error;
use crate::rlp::result::Result;

// Yellow Paper Appendix B: a single byte below 128 is its own encoding.
const SINGLE_BYTE_MAX: u8 = 0x7F;
// Yellow Paper Appendix B, R_b: a byte array shorter than 56 bytes is prefixed by 128 plus
// its length.
const SHORT_BYTES_BASE: u8 = 0x80;
// Yellow Paper Appendix B, R_b: a longer byte array is prefixed by 183 plus the length of
// its length.
const LONG_BYTES_BASE: u8 = 0xB7;
// Yellow Paper Appendix B, R_l: a list payload shorter than 56 bytes is prefixed by 192 plus
// its length.
const SHORT_LIST_BASE: u8 = 0xC0;
// Yellow Paper Appendix B, R_l: a longer list payload is prefixed by 247 plus the length of
// its length.
const LONG_LIST_BASE: u8 = 0xF7;
// Yellow Paper Appendix B: the short forms cover lengths below 56.
const SHORT_MAX: usize = 55;
// Defined by this crate, below the eight length bytes RLP allows: four is the width of usize
// on riscv32, so the accumulator in long_length cannot overflow.
const LENGTH_BYTES_MAX: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item<'a> {
    Bytes(&'a [u8]),
    List(&'a [u8]),
}

#[derive(Debug, Clone, Copy)]
pub struct Items<'a> {
    rest: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bytes,
    List,
}

impl<'a> Item<'a> {
    pub fn bytes(self) -> Result<&'a [u8]> {
        match self {
            Self::Bytes(bytes) => Ok(bytes),
            Self::List(_) => Err(Error::ExpectedBytes),
        }
    }

    pub fn list(self) -> Result<Items<'a>> {
        match self {
            Self::List(payload) => Ok(Items { rest: payload }),
            Self::Bytes(_) => Err(Error::ExpectedList),
        }
    }
}

impl<'a> Iterator for Items<'a> {
    type Item = Result<Item<'a>>;

    // an iterator for items
    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }

        // decode the first payload and return the rest
        match decode(self.rest) {
            Ok((item, rest)) => {
                self.rest = rest;
                Some(Ok(item))
            }
            Err(error) => {
                self.rest = &[];
                Some(Err(error))
            }
        }
    }
}

pub fn decode_exact(input: &[u8]) -> Result<Item<'_>> {
    let (item, rest) = decode(input)?;

    if !rest.is_empty() {
        return Err(Error::TrailingBytes(rest.len()));
    }

    Ok(item)
}

pub fn decode(input: &[u8]) -> Result<(Item<'_>, &[u8])> {
    // extract the tag
    let Some(tag) = input.first().copied() else {
        return Err(Error::Truncated {
            needed: 1,
            found: 0,
        });
    };

    // Extract rlp data
    let (kind, header_len, payload_len) = header(tag, input)?;

    let Some(total) = header_len.checked_add(payload_len) else {
        return Err(Error::LengthOverflow);
    };
    // retrieve encoded payload
    let Some((encoded, rest)) = input.split_at_checked(total) else {
        return Err(Error::Truncated {
            needed: total,
            found: input.len(),
        });
    };

    // extract only the payload
    let payload = encoded.get(header_len..).unwrap_or(&[]);

    let item = match kind {
        Kind::Bytes => {
            if let [only] = payload
                && header_len == 1
                && *only <= SINGLE_BYTE_MAX
            {
                // if the header is 1 and we only have one byte then
                // there should be only that byte without a header
                return Err(Error::NonCanonical);
            }
            // otherwise return the bytes
            Item::Bytes(payload)
        }
        // if its a list return the list
        Kind::List => Item::List(payload),
    };

    Ok((item, rest))
}

/// compute the header from a tag and input payload
fn header(tag: u8, input: &[u8]) -> Result<(Kind, usize, usize)> {
    // a single byte
    if tag <= SINGLE_BYTE_MAX {
        return Ok((Kind::Bytes, 0, 1));
    }

    // if its a short string, then the tag itself holds the length
    if tag <= LONG_BYTES_BASE {
        // perform the reverse operation to compute the tag
        return Ok((Kind::Bytes, 1, usize::from(tag - SHORT_BYTES_BASE)));
    }

    if tag < SHORT_LIST_BASE {
        // compute how big the length bytes are
        let length_len = usize::from(tag - LONG_BYTES_BASE);
        // compute payload length from the length of the length
        let payload_len = long_length(input, length_len)?;
        return Ok((Kind::Bytes, 1 + length_len, payload_len));
    }

    // if its a small list, return it
    if tag <= LONG_LIST_BASE {
        return Ok((Kind::List, 1, usize::from(tag - SHORT_LIST_BASE)));
    }

    // otherwise its a big list and its the same procedure as with long bytes.
    let length_len = usize::from(tag - LONG_LIST_BASE);
    let payload_len = long_length(input, length_len)?;
    Ok((Kind::List, 1 + length_len, payload_len))
}

/// computes the long length from bytes and given length size
fn long_length(input: &[u8], length_len: usize) -> Result<usize> {
    // ensure the requested len is within limits
    if length_len > LENGTH_BYTES_MAX {
        return Err(Error::LengthOverflow);
    }

    // get the bytes of the length
    let Some(length_bytes) = input.get(1..1 + length_len) else {
        return Err(Error::Truncated {
            needed: 1 + length_len,
            found: input.len(),
        });
    };

    // it wouldnt be minimal if we have a leading 0
    if length_bytes.first() == Some(&0) {
        return Err(Error::NonCanonical);
    }

    let mut length = 0usize;
    for byte in length_bytes {
        // so multiply length by 256 and OR in the byte
        length = length << 8 | usize::from(*byte);
    }

    // if it was not supposed to be long length throw error
    if length <= SHORT_MAX {
        return Err(Error::NonCanonical);
    }

    Ok(length)
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::Item;
    use super::decode;
    use super::decode_exact;

    fn items(item: Item<'_>) -> Vec<Item<'_>> {
        item.list()
            .expect("a list")
            .map(|item| item.expect("a valid item"))
            .collect()
    }

    #[test]
    fn the_wiki_string_vectors_decode() {
        assert_eq!(decode_exact(b"\x83dog"), Ok(Item::Bytes(b"dog")));
        assert_eq!(decode_exact(&[0x80]), Ok(Item::Bytes(&[])));
        assert_eq!(decode_exact(&[0x0F]), Ok(Item::Bytes(&[0x0F])));
        assert_eq!(decode_exact(&[0x00]), Ok(Item::Bytes(&[0x00])));
        assert_eq!(
            decode_exact(&[0x82, 0x04, 0x00]),
            Ok(Item::Bytes(&[0x04, 0x00]))
        );
    }

    #[test]
    fn a_fifty_six_byte_string_uses_the_long_form() {
        let mut encoded = vec![0xB8, 0x38];
        encoded.extend_from_slice(&[b'L'; 56]);
        assert_eq!(decode_exact(&encoded), Ok(Item::Bytes(&[b'L'; 56])));
    }

    #[test]
    fn the_wiki_list_vectors_decode() {
        let cat_dog = decode_exact(b"\xc8\x83cat\x83dog").expect("decodes");
        assert_eq!(
            items(cat_dog),
            vec![Item::Bytes(b"cat"), Item::Bytes(b"dog")]
        );

        assert_eq!(decode_exact(&[0xC0]), Ok(Item::List(&[])));

        let nested =
            decode_exact(&[0xC7, 0xC0, 0xC1, 0xC0, 0xC3, 0xC0, 0xC1, 0xC0]).expect("decodes");
        let outer = items(nested);
        assert_eq!(outer.len(), 3);
        assert_eq!(outer[0], Item::List(&[]));
        assert_eq!(items(outer[1]), vec![Item::List(&[])]);
        assert_eq!(items(outer[2]).len(), 2);
    }

    #[test]
    fn decode_returns_the_remainder() {
        let (item, rest) = decode(&[0x0F, 0xAA, 0xBB]).expect("decodes");
        assert_eq!(item, Item::Bytes(&[0x0F]));
        assert_eq!(rest, &[0xAA, 0xBB]);
    }

    #[test]
    fn trailing_bytes_are_rejected_by_the_exact_form() {
        assert_eq!(
            decode_exact(&[0x0F, 0xAA, 0xBB]),
            Err(Error::TrailingBytes(2))
        );
    }

    #[test]
    fn a_single_byte_wrapped_as_a_string_is_not_canonical() {
        assert_eq!(decode_exact(&[0x81, 0x05]), Err(Error::NonCanonical));
        assert_eq!(decode_exact(&[0x81, 0x7F]), Err(Error::NonCanonical));
        assert_eq!(decode_exact(&[0x81, 0x80]), Ok(Item::Bytes(&[0x80])));
    }

    #[test]
    fn a_short_payload_in_the_long_form_is_not_canonical() {
        let mut encoded = vec![0xB8, 0x05];
        encoded.extend_from_slice(b"hello");
        assert_eq!(decode_exact(&encoded), Err(Error::NonCanonical));
    }

    #[test]
    fn a_length_with_a_leading_zero_is_not_canonical() {
        let mut encoded = vec![0xB9, 0x00, 0x38];
        encoded.extend_from_slice(&[b'L'; 56]);
        assert_eq!(decode_exact(&encoded), Err(Error::NonCanonical));
    }

    #[test]
    fn a_length_wider_than_four_bytes_overflows() {
        assert_eq!(
            decode_exact(&[0xBD, 1, 2, 3, 4, 5, 6]),
            Err(Error::LengthOverflow)
        );
        assert_eq!(
            decode_exact(&[0xFF, 1, 2, 3, 4, 5, 6, 7, 8]),
            Err(Error::LengthOverflow)
        );
    }

    #[test]
    fn truncated_input_names_what_it_needed() {
        assert_eq!(
            decode_exact(&[]),
            Err(Error::Truncated {
                needed: 1,
                found: 0
            })
        );
        assert_eq!(
            decode_exact(b"\x83do"),
            Err(Error::Truncated {
                needed: 4,
                found: 3
            })
        );
        assert_eq!(
            decode_exact(&[0xB8]),
            Err(Error::Truncated {
                needed: 2,
                found: 1
            })
        );
    }

    #[test]
    fn asking_for_the_wrong_kind_is_an_error() {
        assert_eq!(
            Item::Bytes(b"x").list().map(|_| ()),
            Err(Error::ExpectedList)
        );
        assert_eq!(Item::List(&[]).bytes(), Err(Error::ExpectedBytes));
    }

    #[test]
    fn a_corrupt_item_inside_a_list_surfaces_through_the_iterator() {
        let list = decode_exact(&[0xC2, 0x81, 0x05]).expect("the list itself decodes");
        let collected: Vec<_> = list.list().expect("a list").collect();
        assert_eq!(collected, vec![Err(Error::NonCanonical)]);
    }
}
