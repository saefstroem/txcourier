use crate::crc32::checksum;
use crate::token::descriptor::DESCRIPTOR_MAX;
use crate::token::error::Error;
use crate::token::result::Result;
use crate::token::table::{MAX_TOKENS, Tokens};

// A four-byte tag, so a blob that is not a token table fails before it is parsed.
const MAGIC: [u8; 4] = *b"RPTK";
// Bumped when the blob layout changes, so an old provisioning is rejected rather than
// misread.
const VERSION: u8 = 1;
// The magic, the version byte and the count byte.
const HEADER_LEN: usize = MAGIC.len() + 2;
// The width of the CRC-32 crc32::checksum returns.
const CHECKSUM_LEN: usize = 4;
// One length byte per descriptor, which caps a descriptor at 255 bytes.
const LENGTH_LEN: usize = 1;

// The header, every slot filled with a longest descriptor, and the CRC32: 1945 bytes. A
// const assert pins it below PROVISIONING_CAPACITY.
pub const BLOB_MAX: usize = HEADER_LEN + MAX_TOKENS * (LENGTH_LEN + DESCRIPTOR_MAX) + CHECKSUM_LEN;

pub fn encode(tokens: &Tokens, out: &mut [u8]) -> Result<usize> {
    let mut writer = Writer { out, len: 0 };

    writer.push(&MAGIC)?;
    writer.push(&[VERSION, tokens.len() as u8])?;

    for descriptor in tokens.iter() {
        writer.push(&[descriptor.len() as u8])?;
        writer.push(descriptor)?;
    }

    let body = writer.out.get(..writer.len).unwrap_or(&[]);
    let crc = checksum(body).to_le_bytes();
    writer.push(&crc)?;

    Ok(writer.len)
}

pub fn decode(bytes: &[u8]) -> Result<Tokens> {
    let Some(body_len) = bytes.len().checked_sub(CHECKSUM_LEN) else {
        return Err(Error::Truncated);
    };
    let Some((body, declared)) = bytes.split_at_checked(body_len) else {
        return Err(Error::Truncated);
    };
    if declared != checksum(body).to_le_bytes() {
        return Err(Error::ChecksumMismatch);
    }

    let Some((header, mut rest)) = body.split_at_checked(HEADER_LEN) else {
        return Err(Error::Truncated);
    };
    let [magic @ .., version, count] = header else {
        return Err(Error::Truncated);
    };
    if magic != MAGIC {
        return Err(Error::BadMagic);
    }
    if *version != VERSION {
        return Err(Error::UnsupportedVersion(*version));
    }

    let mut tokens = Tokens::EMPTY;

    for _ in 0..*count {
        let Some((&len, tail)) = rest.split_first() else {
            return Err(Error::Truncated);
        };
        let Some((descriptor, tail)) = tail.split_at_checked(usize::from(len)) else {
            return Err(Error::Truncated);
        };
        tokens.push(descriptor)?;
        rest = tail;
    }

    if !rest.is_empty() {
        return Err(Error::TrailingBytes(rest.len()));
    }

    Ok(tokens)
}

struct Writer<'a> {
    out: &'a mut [u8],
    len: usize,
}

impl Writer<'_> {
    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        let needed = self.len + bytes.len();
        let Some(slot) = self.out.get_mut(self.len..needed) else {
            return Err(Error::OutputTooSmall {
                needed,
                found: self.out.len(),
            });
        };
        slot.copy_from_slice(bytes);
        self.len = needed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::BLOB_MAX;
    use super::Error;
    use super::decode;
    use super::encode;
    use crate::crc32::checksum;
    use crate::token::descriptor::DESCRIPTOR_MAX;
    use crate::token::table::MAX_TOKENS;
    use crate::token::table::Tokens;

    const USDT: [u8; 20] = [
        0xda, 0xc1, 0x7f, 0x95, 0x8d, 0x2e, 0xe5, 0x23, 0xa2, 0x20, 0x62, 0x06, 0x99, 0x45, 0x97,
        0xc1, 0x3d, 0x83, 0x1e, 0xc7,
    ];

    const PINNED: [u8; 45] = [
        0x52, 0x50, 0x54, 0x4B, 0x01, 0x01, 0x22, 0x01, 0x54, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
        0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0x00,
        0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x30, 0x02, 0xAB, 0xAB, 0xC3, 0xE8, 0x7C, 0xF0,
    ];

    const PINNED_EMPTY: [u8; 10] = [0x52, 0x50, 0x54, 0x4B, 0x01, 0x00, 0x01, 0x0C, 0x2C, 0xE9];

    const FIXED_LEN: usize = 31;

    fn descriptor(ticker: &[u8], address: &[u8; 20], chain_id: u32, len: usize) -> Vec<u8> {
        let signature = len - FIXED_LEN - ticker.len();
        let mut bytes = vec![ticker.len() as u8];
        bytes.extend_from_slice(ticker);
        bytes.extend_from_slice(address);
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&chain_id.to_be_bytes());
        bytes.extend_from_slice(&[0x30, signature as u8]);
        bytes.extend_from_slice(&vec![0xAB; signature]);
        assert_eq!(bytes.len(), len);
        bytes
    }

    fn sample() -> Tokens {
        let mut tokens = Tokens::EMPTY;
        tokens
            .push(&descriptor(b"USDT", &USDT, 1, 103))
            .expect("room");
        tokens
            .push(&descriptor(b"USDC", &[0xBB; 20], 8453, 105))
            .expect("room");
        tokens
    }

    fn encoded(tokens: &Tokens) -> Vec<u8> {
        let mut buffer = [0u8; BLOB_MAX];
        let len = encode(tokens, &mut buffer).expect("fits");
        buffer[..len].to_vec()
    }

    #[test]
    fn a_single_descriptor_encodes_to_exactly_these_bytes() {
        let mut tokens = Tokens::EMPTY;
        tokens
            .push(&descriptor(b"T", &[0xAA; 20], 1, 34))
            .expect("room");
        assert_eq!(encoded(&tokens), PINNED.to_vec());
        assert_eq!(decode(&PINNED), Ok(tokens));
    }

    #[test]
    fn an_empty_table_encodes_to_exactly_these_bytes() {
        assert_eq!(encoded(&Tokens::EMPTY), PINNED_EMPTY.to_vec());
        assert_eq!(decode(&PINNED_EMPTY), Ok(Tokens::EMPTY));
    }

    #[test]
    fn a_record_round_trips() {
        assert_eq!(decode(&encoded(&sample())), Ok(sample()));
    }

    #[test]
    fn a_full_table_of_the_longest_descriptors_fits_in_the_blob() {
        let mut tokens = Tokens::EMPTY;
        for fill in 0..MAX_TOKENS {
            tokens
                .push(&descriptor(b"TKN", &[fill as u8; 20], 1, DESCRIPTOR_MAX))
                .expect("room");
        }
        let bytes = encoded(&tokens);
        assert_eq!(bytes.len(), BLOB_MAX);
        assert_eq!(decode(&bytes), Ok(tokens));
    }

    #[test]
    fn every_single_bit_flip_is_detected() {
        let original = encoded(&sample());
        for index in 0..original.len() {
            for bit in 0..8 {
                let mut corrupted = original.clone();
                corrupted[index] ^= 1 << bit;
                assert!(decode(&corrupted).is_err(), "byte {index} bit {bit}");
            }
        }
    }

    #[test]
    fn a_wrong_magic_or_version_is_named() {
        let mut bytes = encoded(&sample());
        bytes[0] = b'X';
        reseal(&mut bytes);
        assert_eq!(decode(&bytes), Err(Error::BadMagic));

        let mut bytes = encoded(&sample());
        bytes[4] = 2;
        reseal(&mut bytes);
        assert_eq!(decode(&bytes), Err(Error::UnsupportedVersion(2)));
    }

    #[test]
    fn a_count_that_promises_more_than_the_record_holds_is_rejected() {
        let mut bytes = encoded(&sample());
        bytes[5] = 3;
        reseal(&mut bytes);
        assert_eq!(decode(&bytes), Err(Error::Truncated));

        let mut bytes = encoded(&sample());
        bytes[5] = 1;
        reseal(&mut bytes);
        assert!(matches!(decode(&bytes), Err(Error::TrailingBytes(_))));
    }

    #[test]
    fn a_truncated_record_is_rejected() {
        let bytes = encoded(&sample());
        assert_eq!(decode(&bytes[..3]), Err(Error::Truncated));
        assert_eq!(decode(&[]), Err(Error::Truncated));
    }

    #[test]
    fn a_short_buffer_is_refused_when_encoding() {
        let mut buffer = [0u8; 16];
        assert!(matches!(
            encode(&sample(), &mut buffer),
            Err(Error::OutputTooSmall { .. })
        ));
    }

    fn reseal(bytes: &mut [u8]) {
        let len = bytes.len();
        let crc = checksum(&bytes[..len - 4]).to_le_bytes();
        bytes[len - 4..].copy_from_slice(&crc);
    }
}
