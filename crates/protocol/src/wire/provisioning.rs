use crate::store::{TOKEN_TABLE_RECORD_LEN, payload_capacity};
use crate::token::BLOB_MAX;
use crate::wire::bytes::Bytes;
use crate::wire::frame::MAX_PAYLOAD_LEN;

// What one token table record holds: 2048 less the 8-byte record header and the CRC32.
pub const PROVISIONING_CAPACITY: usize = payload_capacity(TOKEN_TABLE_RECORD_LEN);

const _: () = assert!(PROVISIONING_CAPACITY <= MAX_PAYLOAD_LEN);
const _: () = assert!(BLOB_MAX <= PROVISIONING_CAPACITY);

pub type Provisioning = Bytes<PROVISIONING_CAPACITY>;

#[cfg(test)]
mod tests {
    use super::PROVISIONING_CAPACITY;
    use super::Provisioning;
    use crate::token::BLOB_MAX;
    use crate::token::DESCRIPTOR_MAX;
    use crate::token::MAX_TOKENS;
    use crate::token::Tokens;
    use crate::token::encode;

    #[test]
    fn the_largest_possible_token_table_fits_in_one_frame() {
        let mut tokens = Tokens::EMPTY;
        for fill in 0..MAX_TOKENS {
            tokens.push(&[fill as u8; DESCRIPTOR_MAX]).expect("room");
        }

        let mut buffer = [0u8; PROVISIONING_CAPACITY];
        let len = encode(&tokens, &mut buffer).expect("fits");
        assert_eq!(len, BLOB_MAX);
        assert!(len <= PROVISIONING_CAPACITY);
        assert!(Provisioning::from_slice(&buffer[..len]).is_ok());
    }

    #[test]
    fn a_blob_at_capacity_fits_and_one_more_does_not() {
        assert!(Provisioning::from_slice(&[0u8; PROVISIONING_CAPACITY]).is_ok());
        assert!(Provisioning::from_slice(&[0u8; PROVISIONING_CAPACITY + 1]).is_err());
    }
}
